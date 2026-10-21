//! Builds and runs the ffmpeg command line for a conversion.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicI32, Ordering};

use anyhow::{bail, Context, Result};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::db::Profile;
use crate::probe::MediaInfo;

/// Output extension for a given profile and source file.
pub fn output_extension(profile: &Profile, source: &Path) -> String {
    match profile.container.as_str() {
        "mp4" => "mp4".into(),
        "keep" => match source.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()) {
            Some(e) if e == "mp4" || e == "m4v" => "mp4".into(),
            _ => "mkv".into(),
        },
        _ => "mkv".into(),
    }
}

fn video_filter(profile: &Profile, info: &MediaInfo) -> Option<String> {
    let mut filters = Vec::new();
    if profile.max_width > 0 || profile.max_height > 0 {
        let w = if profile.max_width > 0 { format!("'min({},iw)'", profile.max_width) } else { "iw".into() };
        let h = if profile.max_height > 0 { format!("'min({},ih)'", profile.max_height) } else { "ih".into() };
        filters.push(format!(
            "scale=w={w}:h={h}:force_original_aspect_ratio=decrease:force_divisible_by=2"
        ));
    }
    let eight_bit = profile.pix_fmt == "yuv420p";
    if info.hdr && eight_bit {
        // HDR -> SDR tone mapping, otherwise an 8-bit output looks washed out.
        filters.push(
            "zscale=t=linear:npl=100,format=gbrpf32le,zscale=p=bt709,tonemap=tonemap=hable:desat=0,\
             zscale=t=bt709:m=bt709:r=tv"
                .into(),
        );
    }
    if !profile.pix_fmt.is_empty() {
        filters.push(format!("format={}", profile.pix_fmt));
    }
    (!filters.is_empty()).then(|| filters.join(","))
}

pub fn build_args(profile: &Profile, info: &MediaInfo, input: &Path, output: &Path) -> Vec<String> {
    let mp4 = output.extension().and_then(|e| e.to_str()) == Some("mp4");
    let mut a: Vec<String> = ["-hide_banner", "-nostdin", "-y", "-loglevel", "warning", "-stats_period", "2"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    a.extend(["-progress".into(), "pipe:1".into(), "-nostats".into()]);
    a.push("-i".into());
    a.push(input.to_string_lossy().into_owned());
    // Real video streams only (no cover art), every audio track, subtitles and fonts.
    a.extend(["-map", "0:V", "-map", "0:a?"].map(String::from));
    if mp4 {
        a.push("-sn".into());
    } else {
        a.extend(["-map", "0:s?", "-map", "0:t?"].map(String::from));
    }
    a.extend(["-map_metadata", "0", "-map_chapters", "0", "-c", "copy"].map(String::from));

    a.extend(["-c:v".into(), profile.encoder.clone()]);
    match profile.encoder.as_str() {
        "libx265" | "libx264" | "libsvtav1" => {
            if !profile.preset.is_empty() {
                a.extend(["-preset".into(), profile.preset.clone()]);
            }
            a.extend(["-crf".into(), profile.crf.to_string()]);
            if profile.encoder == "libx265" {
                a.extend(["-x265-params".into(), "log-level=error".into()]);
                a.extend(["-tag:v".into(), "hvc1".into()]);
            }
        }
        _ => {}
    }
    if let Some(vf) = video_filter(profile, info) {
        a.extend(["-filter:v".into(), vf]);
    }
    match profile.audio.as_str() {
        "aac" => a.extend(["-c:a", "aac", "-b:a", "192k"].map(String::from)),
        "ac3" => a.extend(["-c:a", "ac3", "-b:a", "640k"].map(String::from)),
        _ => {}
    }
    if mp4 {
        a.extend(["-movflags".into(), "+faststart".into()]);
    }
    a.extend(["-max_muxing_queue_size".into(), "4096".into()]);
    a.extend(profile.extra_args.split_whitespace().map(String::from));
    a.push(output.to_string_lossy().into_owned());
    a
}

#[derive(Debug, Clone, Default)]
pub struct Progress {
    pub percent: f64,
    pub speed: String,
    pub eta_seconds: Option<i64>,
}

pub struct RunOutcome {
    pub success: bool,
    pub log: String,
}

/// Seconds of source encoded so far. ffmpeg's out_time is the position of the
/// slowest output stream: a sparse one (e.g. a forced-subtitle track) keeps it
/// stuck at its last packet, or at N/A, while the video goes on, so the video
/// frame count / fps wins when it is further ahead.
fn position(out_time: f64, frame: f64, fps: f64) -> f64 {
    let by_frames = if fps > 0.0 { frame / fps } else { 0.0 };
    out_time.max(by_frames)
}

/// Runs ffmpeg, publishing the child pid in `pid` (so it can be paused or
/// killed from outside) and reporting progress through `on_progress`.
pub async fn run(
    args: &[String],
    duration: f64,
    fps: f64,
    pid: &AtomicI32,
    mut on_progress: impl FnMut(Progress),
) -> Result<RunOutcome> {
    let mut child = Command::new("ffmpeg")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("starting ffmpeg")?;
    pid.store(child.id().map(|p| p as i32).unwrap_or(0), Ordering::SeqCst);

    let stderr = child.stderr.take().unwrap();
    let log_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        let mut tail: VecDeque<String> = VecDeque::new();
        while let Ok(Some(line)) = lines.next_line().await {
            if tail.len() >= 200 {
                tail.pop_front();
            }
            tail.push_back(line);
        }
        tail.into_iter().collect::<Vec<_>>().join("\n")
    });

    // Position comes from out_time and the video frame count (see `position`);
    // the speed is computed from encoding time (gaps > 10s = suspended).
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut p = Progress::default();
    let (mut out_time, mut frame) = (0.0f64, 0.0f64);
    let mut active = 0.0f64;
    let mut last_tick = std::time::Instant::now();
    while let Some(line) = lines.next_line().await? {
        let Some((k, v)) = line.split_once('=') else { continue };
        let v = v.trim();
        match k {
            // Both are microseconds (ffmpeg's "ms" key is a historical misnomer).
            "out_time_us" | "out_time_ms" => out_time = v.parse::<f64>().map(|us| us / 1e6).unwrap_or(out_time),
            "frame" => frame = v.parse().unwrap_or(frame),
            "progress" => {
                let gap = last_tick.elapsed().as_secs_f64();
                last_tick = std::time::Instant::now();
                if gap < 10.0 {
                    active += gap;
                }
                let done = position(out_time, frame, fps);
                let speed = if active > 0.0 { done / active } else { 0.0 };
                p.speed = if speed > 0.0 { format!("{speed:.3}x") } else { String::new() };
                if duration > 0.0 {
                    p.percent = (done / duration * 100.0).clamp(0.0, 100.0);
                    p.eta_seconds = (speed > 0.0).then(|| ((duration - done).max(0.0) / speed) as i64);
                }
                on_progress(p.clone());
            }
            _ => {}
        }
    }
    let status = child.wait().await?;
    pid.store(0, Ordering::SeqCst);
    let log = log_task.await.unwrap_or_default();
    Ok(RunOutcome { success: status.success(), log })
}

pub async fn version() -> String {
    match Command::new("ffmpeg").arg("-version").output().await {
        Ok(o) => String::from_utf8_lossy(&o.stdout).lines().next().unwrap_or("").to_string(),
        Err(_) => "ffmpeg not found".into(),
    }
}

/// Moves `src` next to `final_path` and atomically renames it into place.
/// Works across filesystems (cache on the system disk, library on another).
pub async fn install_output(src: &Path, final_path: &Path) -> Result<()> {
    let dir = final_path.parent().context("output has no parent dir")?;
    let name = final_path.file_name().context("output has no file name")?.to_string_lossy();
    let tmp: PathBuf = dir.join(format!(".{name}.converterr-tmp"));
    if tokio::fs::rename(src, &tmp).await.is_err() {
        if let Err(e) = tokio::fs::copy(src, &tmp).await {
            let _ = tokio::fs::remove_file(&tmp).await;
            bail!("copying into library failed: {e}");
        }
        let f = tokio::fs::File::open(&tmp).await?;
        f.sync_all().await?;
        let _ = tokio::fs::remove_file(src).await;
    }
    if let Err(e) = tokio::fs::rename(&tmp, final_path).await {
        let _ = tokio::fs::remove_file(&tmp).await;
        bail!("rename into place failed: {e}");
    }
    // Make the rename itself durable, or a power cut could undo it.
    if let Ok(d) = tokio::fs::File::open(dir).await {
        let _ = d.sync_all().await;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_ignores_lagging_subtitles() {
        // BB S03E03: out_time stuck at the last forced subtitle (217 s), video at 1000 s.
        assert_eq!(position(217.0, 24_000.0, 24.0), 1000.0);
        // out_time N/A (0) before the first packet of a sparse stream.
        assert_eq!(position(0.0, 2_400.0, 24.0), 100.0);
        // Normal case and unknown fps: out_time is used.
        assert_eq!(position(500.0, 11_990.0, 24.0), 500.0);
        assert_eq!(position(500.0, 12_000.0, 0.0), 500.0);
    }
}
