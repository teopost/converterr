//! ffprobe wrapper and the "does this file need converting?" decision.

use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use tokio::process::Command;

use crate::db::Profile;

#[derive(Debug, Clone, Default)]
pub struct MediaInfo {
    pub codec: String,
    pub width: i64,
    pub height: i64,
    pub bit_depth: i64,
    pub hdr: bool,
    pub duration: f64,
    pub fps: f64,
}

#[derive(Deserialize)]
struct ProbeOut {
    #[serde(default)]
    streams: Vec<ProbeStream>,
    format: Option<ProbeFormat>,
}

#[derive(Deserialize)]
struct ProbeStream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<i64>,
    height: Option<i64>,
    pix_fmt: Option<String>,
    bits_per_raw_sample: Option<String>,
    color_transfer: Option<String>,
    duration: Option<String>,
    avg_frame_rate: Option<String>,
    r_frame_rate: Option<String>,
    #[serde(default)]
    disposition: Disposition,
}

#[derive(Deserialize, Default)]
struct Disposition {
    #[serde(default)]
    attached_pic: i64,
}

#[derive(Deserialize)]
struct ProbeFormat {
    duration: Option<String>,
}

pub async fn probe(path: &Path) -> Result<MediaInfo> {
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams"])
        .arg(path)
        .kill_on_drop(true)
        .output()
        .await
        .context("running ffprobe")?;
    if !out.status.success() {
        bail!("ffprobe failed: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    let p: ProbeOut = serde_json::from_slice(&out.stdout).context("parsing ffprobe output")?;
    let videos: Vec<&ProbeStream> = p
        .streams
        .iter()
        .filter(|s| s.codec_type.as_deref() == Some("video") && s.disposition.attached_pic == 0)
        .collect();
    let Some(v) = videos.first() else {
        bail!("no video stream");
    };
    let pix_fmt = v.pix_fmt.clone().unwrap_or_default();
    let bit_depth = v
        .bits_per_raw_sample
        .as_deref()
        .and_then(|b| b.parse().ok())
        .unwrap_or_else(|| if pix_fmt.contains("10") { 10 } else if pix_fmt.contains("12") { 12 } else { 8 });
    let duration = p
        .format
        .as_ref()
        .and_then(|f| f.duration.as_deref())
        .or(v.duration.as_deref())
        .and_then(|d| d.parse().ok())
        .unwrap_or(0.0);
    Ok(MediaInfo {
        codec: v.codec_name.clone().unwrap_or_default(),
        width: v.width.unwrap_or(0),
        height: v.height.unwrap_or(0),
        bit_depth,
        hdr: matches!(v.color_transfer.as_deref(), Some("smpte2084") | Some("arib-std-b67")),
        duration,
        fps: parse_rate(v.avg_frame_rate.as_deref())
            .or_else(|| parse_rate(v.r_frame_rate.as_deref()))
            .unwrap_or(0.0),
    })
}

/// Parses ffprobe rates like "24000/1001".
fn parse_rate(r: Option<&str>) -> Option<f64> {
    let (n, d) = r?.split_once('/')?;
    let (n, d): (f64, f64) = (n.parse().ok()?, d.parse().ok()?);
    (n > 0.0 && d > 0.0).then(|| n / d)
}

/// Returns Some(reason) when the profile says the file must be converted.
pub fn needs_conversion(info: &MediaInfo, profile: &Profile) -> Option<String> {
    let codec = info.codec.to_lowercase();
    let target = profile.target_codec();
    let list = profile.source_codec_list();
    if list.is_empty() {
        if !target.is_empty() && codec != target {
            return Some(format!("codec {codec} ≠ {target}"));
        }
    } else if list.iter().any(|c| *c == codec) {
        return Some(format!("codec {codec}"));
    }
    if profile.convert_oversized
        && ((profile.max_width > 0 && info.width > profile.max_width)
            || (profile.max_height > 0 && info.height > profile.max_height))
    {
        return Some(format!("{}x{} > {}x{}", info.width, info.height, profile.max_width, profile.max_height));
    }
    None
}
