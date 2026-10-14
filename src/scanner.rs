//! Library scanner: walks library folders, probes new/changed files and
//! queues the ones the library profile wants converted.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use anyhow::{Context, Result};
use tokio::sync::Semaphore;

use crate::db::{now, FileEntry, Library, Profile};
use crate::probe;
use crate::state::AppState;

/// Files modified more recently than this may still be being copied.
const SETTLE_SECS: u64 = 120;

fn recently_modified(path: &Path) -> bool {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .map(|age| age.as_secs() < SETTLE_SECS)
        .unwrap_or(false)
}

fn is_temp(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with('.') || n.ends_with(".converterr-tmp") || n.ends_with(".partial"))
        .unwrap_or(true)
}

/// Probes one file (unless the cached result is still valid) and queues it if needed.
/// Returns the file entry if a task was queued.
pub async fn check_file(
    state: &AppState,
    library: &Library,
    profile: &Profile,
    path: &Path,
    force: bool,
) -> Result<Option<FileEntry>> {
    let meta = tokio::fs::metadata(path).await.with_context(|| format!("{} not found", path.display()))?;
    let size = meta.len() as i64;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let key = path.to_string_lossy().into_owned();

    let cached = state.db.file(&key)?;
    let entry = match cached {
        Some(c) if !force && c.size == size && c.mtime == mtime && c.library_id == library.id => c,
        _ => {
            let (codec, width, height, bit_depth, duration, needs, reason) = match probe::probe(path).await {
                Ok(info) => {
                    let reason = probe::needs_conversion(&info, profile);
                    (info.codec, info.width, info.height, info.bit_depth, info.duration, reason.is_some(), reason.unwrap_or_default())
                }
                Err(e) => (String::new(), 0, 0, 0, 0.0, false, format!("probe failed: {e:#}")),
            };
            let entry = FileEntry {
                path: key.clone(),
                library_id: library.id,
                size,
                mtime,
                codec,
                width,
                height,
                bit_depth,
                duration,
                needs_convert: needs,
                reason,
                checked_at: now(),
            };
            state.db.upsert_file(&entry)?;
            entry
        }
    };

    if !entry.needs_convert || (!force && state.db.has_finished_task(&key)?) {
        return Ok(None);
    }
    let queued = state.db.enqueue(library.id, &key, size, &entry.codec, if force { 10 } else { 0 })?.is_some();
    if queued {
        state.notify_queue();
    }
    Ok(queued.then_some(entry))
}

/// One line describing a probed file, e.g. "av1 1920×1080 10-bit · 1.42 GB · codec av1".
pub fn describe(f: &FileEntry) -> String {
    let depth = if f.bit_depth > 8 { format!(" {}-bit", f.bit_depth) } else { String::new() };
    format!("{} {}×{}{} · {} · {}", f.codec, f.width, f.height, depth, crate::worker::human(f.size), f.reason)
}

pub async fn scan_library(state: AppState, library_id: i64) -> Result<()> {
    {
        let mut scanning = state.scanning.lock().unwrap_or_else(|e| e.into_inner());
        if !scanning.insert(library_id) {
            return Ok(());
        }
    }
    let result = do_scan(&state, library_id).await;
    state.scanning.lock().unwrap_or_else(|e| e.into_inner()).remove(&library_id);
    if let Err(e) = &result {
        state.log("error", &format!("Scan of library {library_id} failed: {e:#}"));
    }
    result
}

async fn do_scan(state: &AppState, library_id: i64) -> Result<()> {
    let library = state.db.library(library_id)?.context("library not found")?;
    let profile = state.db.profile(library.profile_id)?.context("profile not found")?;
    let settings = state.db.settings()?;
    state.log("info", &format!("Scanning library '{}' ({})", library.name, library.path));

    let root = PathBuf::from(&library.path);
    let lib = library.clone();
    let files: Vec<PathBuf> = tokio::task::spawn_blocking(move || {
        walkdir::WalkDir::new(&root)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .map(|e| e.into_path())
            .filter(|p| !is_temp(p) && lib.accepts(p) && !recently_modified(p))
            .collect()
    })
    .await?;

    let sem = Arc::new(Semaphore::new(settings.probe_concurrency.clamp(1, 16) as usize));
    let mut handles = Vec::with_capacity(files.len());
    for path in files.iter().cloned() {
        let permit = sem.clone().acquire_owned().await?;
        let (st, lib, prof) = (state.clone(), library.clone(), profile.clone());
        handles.push(tokio::spawn(async move {
            let r = check_file(&st, &lib, &prof, &path, false).await;
            drop(permit);
            if let Err(e) = &r {
                st.log("warn", &format!("{}: {e:#}", path.display()));
            }
            r.ok().flatten()
        }));
    }
    let mut queued_files = Vec::new();
    for h in handles {
        if let Ok(Some(f)) = h.await {
            queued_files.push(f);
        }
    }
    let queued = queued_files.len();
    if queued > 0 {
        notify_queued_batch(state, &library, &queued_files);
    }

    // Forget files that disappeared from disk.
    let present: std::collections::HashSet<String> =
        files.iter().map(|p| p.to_string_lossy().into_owned()).collect();
    for p in state.db.library_file_paths(library_id)? {
        if !present.contains(&p) {
            state.db.delete_file(&p)?;
        }
    }

    state.log(
        "info",
        &format!("Scan of '{}' done: {} files, {} queued", library.name, files.len(), queued),
    );
    Ok(())
}

fn notify_queued_batch(state: &AppState, library: &Library, files: &[FileEntry]) {
    use crate::notify::{event, file_name, html, Event};
    if files.len() == 1 {
        let f = &files[0];
        event(
            state,
            Event::Queued,
            "File Queued",
            format!("{}\n{}\nLibrary: {}", html(&file_name(Path::new(&f.path))), html(&describe(f)), html(&library.name)),
        );
        return;
    }
    let total: i64 = files.iter().map(|f| f.size).sum();
    let mut body = format!(
        "{} files to convert in {} ({})\n",
        files.len(),
        html(&library.name),
        crate::worker::human(total)
    );
    for f in files.iter().take(10) {
        body.push_str(&format!("• {}\n", html(&file_name(Path::new(&f.path)))));
    }
    if files.len() > 10 {
        body.push_str(&format!("…and {} more", files.len() - 10));
    }
    event(state, Event::Queued, "Files Queued", body);
}

/// Periodic scanner driven by `scan_interval_minutes`.
pub fn start(state: AppState) {
    tokio::spawn(async move {
        let mut last = std::time::Instant::now();
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            let interval = state.db.settings().map(|s| s.scan_interval_minutes).unwrap_or(0);
            if interval <= 0 || last.elapsed() < Duration::from_secs(interval as u64 * 60) {
                continue;
            }
            last = std::time::Instant::now();
            for lib in state.db.libraries().unwrap_or_default() {
                if lib.enabled {
                    let _ = scan_library(state.clone(), lib.id).await;
                }
            }
        }
    });
}
