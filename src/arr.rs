//! Sonarr / Radarr integration: rescan after a conversion, path mapping.

use std::path::Path;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use crate::db::{Library, Settings};

/// Library path -> path as seen by Sonarr/Radarr.
pub fn to_arr_path(library: &Library, local: &Path) -> String {
    let local = local.to_string_lossy();
    if library.arr_path.is_empty() {
        return local.into_owned();
    }
    match local.strip_prefix(library.path.trim_end_matches('/')) {
        Some(rest) => format!("{}{}", library.arr_path.trim_end_matches('/'), rest),
        None => local.into_owned(),
    }
}

/// Path as seen by Sonarr/Radarr -> local library path, if it belongs to the library.
pub fn from_arr_path(library: &Library, remote: &str) -> Option<String> {
    let base = if library.arr_path.is_empty() { &library.path } else { &library.arr_path };
    let base = base.trim_end_matches('/');
    let rest = remote.strip_prefix(base)?;
    if !(rest.is_empty() || rest.starts_with('/')) {
        return None;
    }
    Some(format!("{}{}", library.path.trim_end_matches('/'), rest))
}

fn client() -> reqwest::Client {
    reqwest::Client::builder().timeout(Duration::from_secs(30)).build().unwrap_or_default()
}

async fn get(url: &str, key: &str, path: &str) -> Result<Value> {
    let r = client()
        .get(format!("{}/api/v3/{path}", url.trim_end_matches('/')))
        .header("X-Api-Key", key)
        .send()
        .await?;
    if !r.status().is_success() {
        bail!("{} returned {}", path, r.status());
    }
    Ok(r.json().await?)
}

async fn command(url: &str, key: &str, body: Value) -> Result<()> {
    let r = client()
        .post(format!("{}/api/v3/command", url.trim_end_matches('/')))
        .header("X-Api-Key", key)
        .json(&body)
        .send()
        .await?;
    if !r.status().is_success() {
        bail!("command returned {}", r.status());
    }
    Ok(())
}

/// Finds the series/movie whose folder contains `arr_file` and returns its id.
fn find_owner(items: &Value, arr_file: &str) -> Option<i64> {
    items
        .as_array()?
        .iter()
        .filter_map(|i| Some((i.get("id")?.as_i64()?, i.get("path")?.as_str()?)))
        .filter(|(_, p)| arr_file.starts_with(&format!("{}/", p.trim_end_matches('/'))))
        .max_by_key(|(_, p)| p.len())
        .map(|(id, _)| id)
}

pub async fn notify_library(settings: &Settings, library: &Library, file: &Path) -> Result<()> {
    let arr_file = to_arr_path(library, file);
    match library.notify.as_str() {
        "sonarr" => {
            let (url, key) = (&settings.sonarr_url, &settings.sonarr_api_key);
            if url.is_empty() {
                bail!("Sonarr URL not configured");
            }
            let series = get(url, key, "series").await?;
            let id = find_owner(&series, &arr_file).with_context(|| format!("no Sonarr series contains {arr_file}"))?;
            command(url, key, json!({"name": "RescanSeries", "seriesId": id})).await
        }
        "radarr" => {
            let (url, key) = (&settings.radarr_url, &settings.radarr_api_key);
            if url.is_empty() {
                bail!("Radarr URL not configured");
            }
            let movies = get(url, key, "movie").await?;
            let id = find_owner(&movies, &arr_file).with_context(|| format!("no Radarr movie contains {arr_file}"))?;
            command(url, key, json!({"name": "RescanMovie", "movieIds": [id]})).await
        }
        _ => Ok(()),
    }
}

/// Connection test used by the settings page.
pub async fn test(url: &str, key: &str) -> Result<String> {
    let v = get(url, key, "system/status").await?;
    Ok(format!(
        "{} {}",
        v.get("appName").and_then(|a| a.as_str()).unwrap_or("?"),
        v.get("version").and_then(|a| a.as_str()).unwrap_or("?")
    ))
}

/// Extracts imported file paths from a Sonarr/Radarr webhook payload.
pub fn webhook_paths(payload: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let event = payload.get("eventType").and_then(|e| e.as_str()).unwrap_or("");
    if event != "Download" {
        return out;
    }
    // Sonarr: episodeFile.path (single) or episodeFiles[].path (season pack import)
    // Radarr: movieFile.path; older payloads only carry relativePath + series/movie folder.
    for (file_key, folder) in [
        ("episodeFile", payload.pointer("/series/path")),
        ("movieFile", payload.pointer("/movie/folderPath")),
    ] {
        if let Some(f) = payload.get(file_key) {
            if let Some(p) = f.get("path").and_then(|p| p.as_str()) {
                out.push(p.to_string());
            } else if let (Some(rel), Some(dir)) =
                (f.get("relativePath").and_then(|p| p.as_str()), folder.and_then(|d| d.as_str()))
            {
                out.push(format!("{}/{}", dir.trim_end_matches('/'), rel));
            }
        }
    }
    if let Some(files) = payload.get("episodeFiles").and_then(|f| f.as_array()) {
        for f in files {
            if let Some(p) = f.get("path").and_then(|p| p.as_str()) {
                if !out.iter().any(|o| o == p) {
                    out.push(p.to_string());
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lib() -> Library {
        Library { path: "/library/tv".into(), arr_path: "/tv".into(), ..Default::default() }
    }

    #[test]
    fn mapping() {
        let l = lib();
        assert_eq!(to_arr_path(&l, Path::new("/library/tv/Show/S01/e1.mkv")), "/tv/Show/S01/e1.mkv");
        assert_eq!(from_arr_path(&l, "/tv/Show/S01/e1.mkv").as_deref(), Some("/library/tv/Show/S01/e1.mkv"));
        assert_eq!(from_arr_path(&l, "/tvshows/x.mkv"), None);
    }

    #[test]
    fn owner() {
        let items = json!([{"id": 1, "path": "/tv/Starlight"}, {"id": 2, "path": "/tv/Starlight Harbor"}]);
        assert_eq!(find_owner(&items, "/tv/Starlight Harbor/S01/e1.mkv"), Some(2));
    }

    #[test]
    fn webhook() {
        let p = json!({"eventType": "Download", "series": {"path": "/tv/Show"}, "episodeFile": {"relativePath": "S01/e1.mkv"}});
        assert_eq!(webhook_paths(&p), vec!["/tv/Show/S01/e1.mkv"]);
        let p = json!({"eventType": "Test"});
        assert!(webhook_paths(&p).is_empty());
    }
}
