//! Worker pool: takes queued tasks, runs ffmpeg, verifies and replaces files.
//! A supervisor applies the pause switch / time window by suspending ffmpeg
//! (SIGSTOP) instead of killing it, so long encodes resume where they stopped.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use chrono::{Local, NaiveTime};
use serde::Serialize;

use crate::db::{FileEntry, Settings};
use crate::state::AppState;
use crate::notify::{self, html, Event};
use crate::{arr, ffmpeg, probe};

pub const MAX_WORKERS: usize = 8;
/// Index of the extra slot that only runs forced tasks.
pub const FORCE_SLOT: usize = MAX_WORKERS;

#[derive(Default)]
pub struct WorkerSlot {
    pub task_id: AtomicI64,
    pub pid: AtomicI32,
    pub suspended: AtomicBool,
    pub cancel: AtomicBool,
    /// The running task was forced: keep it running outside the time window.
    pub forced: AtomicBool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerInfo {
    pub id: usize,
    pub enabled: bool,
    pub task_id: Option<i64>,
    pub suspended: bool,
    pub forced: bool,
}

/// Is a time inside the [start, end) window? Handles windows across midnight.
pub fn in_window(now: NaiveTime, start: &str, end: &str) -> bool {
    let parse = |s: &str| NaiveTime::parse_from_str(s, "%H:%M").ok();
    let (Some(s), Some(e)) = (parse(start), parse(end)) else { return true };
    if s == e {
        true
    } else if s < e {
        now >= s && now < e
    } else {
        now >= s || now < e
    }
}

pub fn should_run(s: &Settings) -> bool {
    !s.paused && (!s.schedule_enabled || in_window(Local::now().time(), &s.schedule_start, &s.schedule_end))
}

fn signal(pid: i32, sig: i32) {
    if pid > 0 {
        // SAFETY: plain kill(2) on a pid we spawned.
        unsafe {
            libc::kill(pid, sig);
        }
    }
}

pub fn workers_info(state: &AppState) -> Vec<WorkerInfo> {
    let n = state.db.settings().map(|s| s.workers).unwrap_or(1) as usize;
    state
        .workers
        .iter()
        .enumerate()
        .filter(|(i, slot)| (*i < n && *i != FORCE_SLOT) || slot.task_id.load(Ordering::SeqCst) > 0)
        .map(|(i, slot)| {
            let t = slot.task_id.load(Ordering::SeqCst);
            WorkerInfo {
                id: i + 1,
                enabled: i < n || i == FORCE_SLOT,
                task_id: (t > 0).then_some(t),
                suspended: slot.suspended.load(Ordering::SeqCst),
                forced: slot.forced.load(Ordering::SeqCst),
            }
        })
        .collect()
}

/// Cancels a running task. Returns false if no worker is running it.
pub fn cancel_task(state: &AppState, task_id: i64) -> bool {
    for slot in state.workers.iter() {
        if slot.task_id.load(Ordering::SeqCst) == task_id {
            slot.cancel.store(true, Ordering::SeqCst);
            let pid = slot.pid.load(Ordering::SeqCst);
            signal(pid, libc::SIGCONT);
            signal(pid, libc::SIGKILL);
            return true;
        }
    }
    false
}

/// Removes what a stop at the wrong moment leaves behind. Must run before the
/// workers start: every `task-N` cache dir belongs to an interrupted task (it
/// restarts from scratch). Half-copied `.converterr-tmp` files in the libraries
/// are removed in the background, only if older than this start.
pub async fn cleanup_leftovers(state: &AppState) {
    let Ok(settings) = state.db.settings() else { return };
    let mut removed = 0;
    if let Ok(mut dir) = tokio::fs::read_dir(&settings.cache_dir).await {
        while let Ok(Some(e)) = dir.next_entry().await {
            let name = e.file_name().to_string_lossy().into_owned();
            let is_task = name.strip_prefix("task-").is_some_and(|n| n.parse::<i64>().is_ok());
            if is_task && tokio::fs::remove_dir_all(e.path()).await.is_ok() {
                removed += 1;
            }
        }
    }
    if removed > 0 {
        state.log("info", &format!("Removed {removed} leftover cache dir(s)"));
    }

    let started = std::time::SystemTime::now();
    let roots: Vec<PathBuf> = state.db.libraries().unwrap_or_default().into_iter().map(|l| PathBuf::from(l.path)).collect();
    let st = state.clone();
    tokio::task::spawn_blocking(move || {
        for root in roots {
            let temps = walkdir::WalkDir::new(&root)
                .follow_links(true)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file() && e.file_name().to_string_lossy().ends_with(".converterr-tmp"))
                .filter(|e| e.metadata().ok().and_then(|m| m.modified().ok()).is_some_and(|t| t < started));
            for e in temps {
                match std::fs::remove_file(e.path()) {
                    Ok(()) => st.log("warn", &format!("Removed leftover {}", e.path().display())),
                    Err(err) => st.log("warn", &format!("Cannot remove leftover {}: {err}", e.path().display())),
                }
            }
        }
    });
}

pub fn start(state: AppState) {
    for idx in 0..=MAX_WORKERS {
        let st = state.clone();
        tokio::spawn(async move { worker_loop(st, idx).await });
    }
    tokio::spawn(supervisor(state));
}

async fn supervisor(state: AppState) {
    let mut was_running = None;
    loop {
        if let Ok(s) = state.db.settings() {
            let run_normal = should_run(&s);
            if was_running != Some(run_normal) {
                state.log("info", if run_normal { "Workers resumed" } else { "Workers paused" });
                was_running = Some(run_normal);
            }
            for slot in state.workers.iter() {
                // Forced tasks ignore the time window, only the pause switch stops them.
                let run = if slot.forced.load(Ordering::SeqCst) { !s.paused } else { run_normal };
                let pid = slot.pid.load(Ordering::SeqCst);
                if pid <= 0 {
                    slot.suspended.store(false, Ordering::SeqCst);
                    continue;
                }
                let suspended = slot.suspended.load(Ordering::SeqCst);
                if !run && !suspended {
                    signal(pid, libc::SIGSTOP);
                    slot.suspended.store(true, Ordering::SeqCst);
                } else if run && suspended {
                    signal(pid, libc::SIGCONT);
                    slot.suspended.store(false, Ordering::SeqCst);
                }
            }
        }
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(10)) => {}
            _ = state.wake.notified() => {}
        }
    }
}

async fn worker_loop(state: AppState, idx: usize) {
    let slot = state.workers[idx].clone();
    loop {
        // Regular workers take any task inside the time window; the force slot
        // takes forced tasks at any time (unless paused), so a forced file starts
        // even while the regular worker holds a suspended long encode.
        let claim = match state.db.settings() {
            Ok(s) if idx == FORCE_SLOT => (!s.paused).then_some(true),
            Ok(s) => ((idx as i64) < s.workers && should_run(&s)).then_some(false),
            Err(_) => None,
        };
        let task = match claim {
            Some(forced_only) => state.db.claim_next(forced_only).ok().flatten(),
            None => None,
        };
        let Some(task) = task else {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(5)) => {}
                _ = state.queue_changed.notified() => {}
            }
            continue;
        };
        slot.task_id.store(task.id, Ordering::SeqCst);
        slot.cancel.store(false, Ordering::SeqCst);
        slot.forced.store(task.force, Ordering::SeqCst);
        state.log("info", &format!("Worker {}: starting {}", idx + 1, task.path));
        let mut fail_log = String::new();
        let result = process(&state, &slot, task.id, task.library_id, Path::new(&task.path), &mut fail_log).await;
        let cancelled = slot.cancel.load(Ordering::SeqCst);
        match result {
            Ok(r) => {
                let _ = state.db.task_finish(task.id, &r.status, &r.message, r.size_after, &r.codec_after, &r.output, &r.log);
                state.log("info", &format!("Worker {}: {} {} {}", idx + 1, r.status, task.path, r.message));
            }
            Err(e) => {
                let status = if cancelled { "cancelled" } else { "failed" };
                let msg = format!("{e:#}");
                let _ = state.db.task_finish(task.id, status, &msg, None, "", "", &fail_log);
                if !cancelled {
                    notify::event(
                        &state,
                        Event::Failed,
                        "Conversion Failed",
                        format!("{}\n{}", html(&notify::file_name(Path::new(&task.path))), html(&msg)),
                    );
                }
                state.log(if cancelled { "info" } else { "error" }, &format!("Worker {}: {status} {}: {msg}", idx + 1, task.path));
            }
        }
        slot.task_id.store(0, Ordering::SeqCst);
        slot.pid.store(0, Ordering::SeqCst);
        slot.suspended.store(false, Ordering::SeqCst);
        slot.forced.store(false, Ordering::SeqCst);
    }
}

fn fail<T>(log_out: &mut String, msg: impl Into<String>, log: String) -> Result<T> {
    *log_out = log;
    bail!(msg.into())
}

struct Done {
    status: String,
    message: String,
    size_after: Option<i64>,
    codec_after: String,
    output: String,
    log: String,
}

async fn process(
    state: &AppState,
    slot: &Arc<WorkerSlot>,
    task_id: i64,
    library_id: i64,
    input: &Path,
    log_out: &mut String,
) -> Result<Done> {
    let started = std::time::Instant::now();
    let settings = state.db.settings()?;
    let library = state.db.library(library_id)?.context("library no longer exists")?;
    let profile = state.db.profile(library.profile_id)?.context("profile no longer exists")?;

    let meta = tokio::fs::metadata(input).await.with_context(|| format!("{} not found", input.display()))?;
    let size_before = meta.len() as i64;
    let info = probe::probe(input).await?;
    state.db.task_set_codec(task_id, &info.codec, size_before)?;

    let ext = ffmpeg::output_extension(&profile, input);
    let final_path = input.with_extension(&ext);
    if final_path != input && tokio::fs::try_exists(&final_path).await.unwrap_or(false) {
        // A previous run may have stopped between installing the output and
        // deleting the original: if the existing file is that output, finish the job.
        return match probe::probe(&final_path).await {
            Ok(out) if out.codec == profile.target_codec() && durations_match(info.duration, out.duration) => {
                finish_interrupted(state, &settings, &library, input, &final_path, &out).await
            }
            _ => bail!("{} already exists, refusing to overwrite it", final_path.display()),
        };
    }
    let stem = input.file_stem().context("bad file name")?.to_string_lossy().into_owned();
    let work_dir = PathBuf::from(&settings.cache_dir).join(format!("task-{task_id}"));
    tokio::fs::create_dir_all(&work_dir).await.context("creating cache dir")?;
    let tmp_out = work_dir.join(format!("{stem}.{ext}"));
    let args = ffmpeg::build_args(&profile, &info, input, &tmp_out);

    let db = state.db.clone();
    let mut last_write = std::time::Instant::now() - Duration::from_secs(10);
    let outcome = ffmpeg::run(&args, info.duration, info.fps, &slot.pid, |p| {
        if last_write.elapsed() >= Duration::from_secs(2) {
            last_write = std::time::Instant::now();
            let _ = db.task_progress(task_id, p.percent, &p.speed, p.eta_seconds);
        }
    })
    .await;
    let cleanup = || async { let _ = tokio::fs::remove_dir_all(&work_dir).await; };

    let outcome = match outcome {
        Ok(o) => o,
        Err(e) => {
            cleanup().await;
            return Err(e);
        }
    };
    let log = format!("$ ffmpeg {}\n\n{}", shell_join(&args), outcome.log);
    if slot.cancel.load(Ordering::SeqCst) {
        cleanup().await;
        return fail(log_out, "cancelled by user", log);
    }
    if !outcome.success {
        cleanup().await;
        let last = outcome.log.lines().last().unwrap_or("ffmpeg failed").to_string();
        return fail(log_out, format!("ffmpeg failed: {last}"), log);
    }

    // Verify the result before touching the original.
    let out_info = match probe::probe(&tmp_out).await {
        Ok(i) => i,
        Err(e) => {
            cleanup().await;
            return fail(log_out, format!("output not readable: {e:#}"), log);
        }
    };
    if !durations_match(info.duration, out_info.duration) {
        cleanup().await;
        return fail(log_out, 
            format!("duration mismatch: source {:.1}s, output {:.1}s", info.duration, out_info.duration),
            log,
        );
    }
    let size_after = tokio::fs::metadata(&tmp_out).await?.len() as i64;

    if profile.skip_if_larger && size_after >= size_before {
        cleanup().await;
        // Remember the decision so the scanner doesn't queue it again.
        mark_file(state, library_id, input, &info, size_before, "converted output was larger").await;
        notify::event(
            state,
            Event::Completed,
            "Conversion Skipped",
            format!(
                "{}\nOutput not smaller ({} → {}), original kept",
                html(&notify::file_name(input)),
                human(size_before),
                human(size_after)
            ),
        );
        return Ok(Done {
            status: "skipped".into(),
            message: format!("output not smaller ({} → {}), original kept", human(size_before), human(size_after)),
            size_after: Some(size_after),
            codec_after: out_info.codec,
            output: String::new(),
            log,
        });
    }

    if let Err(e) = ffmpeg::install_output(&tmp_out, &final_path).await {
        cleanup().await;
        return fail(log_out, format!("{e:#}"), log);
    }
    if final_path != input {
        let _ = tokio::fs::remove_file(input).await;
        let _ = state.db.delete_file(&input.to_string_lossy());
    }
    cleanup().await;

    // Refresh the file cache with the new media info.
    if let Ok(new_info) = probe::probe(&final_path).await {
        mark_file(state, library_id, &final_path, &new_info, size_after, "").await;
    }

    if let Err(e) = arr::notify_library(&settings, &library, &final_path).await {
        state.log("warn", &format!("{} notify failed: {e:#}", library.notify));
    }

    let saved = if size_before > 0 { 100.0 * (1.0 - size_after as f64 / size_before as f64) } else { 0.0 };
    notify::event(
        state,
        Event::Completed,
        "File Converted",
        format!(
            "{}\n{} {}×{} → {} {}×{}\n{} → {} (-{:.0}%)\nLibrary: {} · took {}",
            html(&notify::file_name(&final_path)),
            info.codec,
            info.width,
            info.height,
            out_info.codec,
            out_info.width,
            out_info.height,
            human(size_before),
            human(size_after),
            saved,
            html(&library.name),
            elapsed(started.elapsed().as_secs())
        ),
    );

    Ok(Done {
        status: "completed".into(),
        message: format!("{} → {}", human(size_before), human(size_after)),
        size_after: Some(size_after),
        codec_after: out_info.codec,
        output: final_path.to_string_lossy().into_owned(),
        log,
    })
}

/// Same length within 1% (at least 2s); unknown source duration always matches.
fn durations_match(source: f64, output: f64) -> bool {
    source <= 0.0 || (output - source).abs() <= (source * 0.01).max(2.0)
}

/// Completes a conversion whose output was already installed when the
/// container stopped: removes the original and tells Sonarr/Radarr.
async fn finish_interrupted(
    state: &AppState,
    settings: &Settings,
    library: &crate::db::Library,
    input: &Path,
    final_path: &Path,
    out: &probe::MediaInfo,
) -> Result<Done> {
    tokio::fs::remove_file(input).await.with_context(|| format!("removing {}", input.display()))?;
    let _ = state.db.delete_file(&input.to_string_lossy());
    let size_after = tokio::fs::metadata(final_path).await.map(|m| m.len() as i64).ok();
    mark_file(state, library.id, final_path, out, size_after.unwrap_or(0), "").await;
    if let Err(e) = arr::notify_library(settings, library, final_path).await {
        state.log("warn", &format!("{} notify failed: {e:#}", library.notify));
    }
    state.log("warn", &format!("{} was already converted by an interrupted run: original removed", final_path.display()));
    Ok(Done {
        status: "completed".into(),
        message: "interrupted conversion completed, original removed".into(),
        size_after,
        codec_after: out.codec.clone(),
        output: final_path.to_string_lossy().into_owned(),
        log: String::new(),
    })
}

async fn mark_file(state: &AppState, library_id: i64, path: &Path, info: &probe::MediaInfo, size: i64, reason: &str) {
    let mtime = tokio::fs::metadata(path)
        .await
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let _ = state.db.upsert_file(&FileEntry {
        path: path.to_string_lossy().into_owned(),
        library_id,
        size,
        mtime,
        codec: info.codec.clone(),
        width: info.width,
        height: info.height,
        bit_depth: info.bit_depth,
        duration: info.duration,
        needs_convert: false,
        reason: reason.into(),
        checked_at: crate::db::now(),
    });
}

fn shell_join(args: &[String]) -> String {
    args.iter()
        .map(|a| if a.contains(|c: char| c.is_whitespace() || c == '\'') { format!("\"{a}\"") } else { a.clone() })
        .collect::<Vec<_>>()
        .join(" ")
}

fn elapsed(secs: u64) -> String {
    let (h, m) = (secs / 3600, (secs % 3600) / 60);
    if h > 0 {
        format!("{h}h {m:02}m")
    } else {
        format!("{m}m {:02}s", secs % 60)
    }
}

pub fn human(bytes: i64) -> String {
    let b = bytes as f64;
    if b >= 1024f64.powi(3) {
        format!("{:.2} GB", b / 1024f64.powi(3))
    } else if b >= 1024f64.powi(2) {
        format!("{:.1} MB", b / 1024f64.powi(2))
    } else {
        format!("{:.0} KB", b / 1024.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window() {
        let t = |h, m| NaiveTime::from_hms_opt(h, m, 0).unwrap();
        assert!(in_window(t(2, 0), "01:00", "08:00"));
        assert!(!in_window(t(9, 0), "01:00", "08:00"));
        assert!(in_window(t(23, 30), "22:00", "06:00"));
        assert!(in_window(t(5, 0), "22:00", "06:00"));
        assert!(!in_window(t(12, 0), "22:00", "06:00"));
    }

    #[test]
    fn duration_tolerance() {
        assert!(durations_match(2837.0, 2840.0));
        assert!(!durations_match(2837.0, 2700.0));
        assert!(durations_match(60.0, 61.9));
        assert!(!durations_match(60.0, 62.5));
        assert!(durations_match(0.0, 123.0));
    }
}
