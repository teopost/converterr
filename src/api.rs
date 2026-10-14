//! HTTP API (`/api/v1`) and the embedded web UI.

use std::path::{Path as FsPath, PathBuf};

use axum::extract::{Path, Query, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::db::{Library, Profile, Settings};
use crate::state::AppState;
use crate::{arr, notify, scanner, worker};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

pub struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "message": self.1 }))).into_response()
    }
}

impl<E: Into<anyhow::Error>> From<E> for ApiError {
    fn from(e: E) -> Self {
        ApiError(StatusCode::BAD_REQUEST, format!("{:#}", e.into()))
    }
}

fn not_found(what: &str) -> ApiError {
    ApiError(StatusCode::NOT_FOUND, format!("{what} not found"))
}

type ApiResult<T> = Result<T, ApiError>;

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/system/status", get(status))
        .route("/settings", get(get_settings).put(put_settings))
        .route("/profile", get(list_profiles).post(create_profile))
        .route("/profile/{id}", put(update_profile).delete(delete_profile))
        .route("/library", get(list_libraries).post(create_library))
        .route("/library/scan", post(scan_all))
        .route("/library/{id}", put(update_library).delete(delete_library))
        .route("/library/{id}/scan", post(scan_one))
        .route("/library/{id}/files", get(library_files))
        .route("/queue", get(queue).post(queue_add))
        .route("/queue/{id}", axum::routing::delete(queue_remove))
        .route("/queue/{id}/top", post(queue_top))
        .route("/queue/force", post(queue_force))
        .route("/history", get(history).delete(clear_history))
        .route("/history/{id}/retry", post(retry))
        .route("/task/{id}/log", get(task_log))
        .route("/workers", get(workers))
        .route("/workers/pause", post(pause))
        .route("/workers/resume", post(resume))
        .route("/log", get(logs))
        .route("/filesystem", get(filesystem))
        .route("/test/arr", post(test_arr))
        .route("/test/telegram", post(test_telegram))
        .route("/webhook", post(webhook))
        .layer(middleware::from_fn_with_state(state.clone(), api_auth));

    Router::new()
        .nest("/api/v1", api)
        .route("/", get(index))
        .route("/app.js", get(|| async { asset("application/javascript", include_str!("web/app.js")) }))
        .route("/app.css", get(|| async { asset("text/css", include_str!("web/app.css")) }))
        .route("/logo.svg", get(|| async { asset("image/svg+xml", include_str!("web/logo.svg")) }))
        .fallback(get(index))
        .layer(middleware::from_fn_with_state(state.clone(), ui_auth))
        .with_state(state)
}

fn asset(ct: &'static str, body: &'static str) -> Response {
    ([(header::CONTENT_TYPE, ct), (header::CACHE_CONTROL, "no-cache")], body).into_response()
}

async fn index(State(st): State<AppState>) -> ApiResult<Html<String>> {
    let s = st.db.settings()?;
    let boot = json!({ "apiKey": s.api_key, "version": VERSION }).to_string();
    Ok(Html(include_str!("web/index.html").replace("/*BOOT*/{}", &boot)))
}

// ---------------------------------------------------------------------------
// Auth: API key for /api (header X-Api-Key or ?apikey=), optional Basic auth for the UI.
// ---------------------------------------------------------------------------

fn query_param<'a>(req: &'a Request, name: &str) -> Option<&'a str> {
    req.uri().query()?.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k.eq_ignore_ascii_case(name)).then_some(v)
    })
}

async fn api_auth(State(st): State<AppState>, req: Request, next: Next) -> Response {
    let key = st.db.settings().map(|s| s.api_key).unwrap_or_default();
    let given = req
        .headers()
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .or_else(|| query_param(&req, "apikey"));
    if key.is_empty() || given == Some(key.as_str()) {
        next.run(req).await
    } else {
        ApiError(StatusCode::UNAUTHORIZED, "invalid or missing API key".into()).into_response()
    }
}

fn basic_ok(headers: &HeaderMap, s: &Settings) -> bool {
    use base64_lite::decode;
    let Some(v) = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()) else { return false };
    let Some(b64) = v.strip_prefix("Basic ") else { return false };
    let Some(raw) = decode(b64.trim()) else { return false };
    raw == format!("{}:{}", s.auth_username, s.auth_password)
}

async fn ui_auth(State(st): State<AppState>, req: Request, next: Next) -> Response {
    if req.uri().path().starts_with("/api/") {
        return next.run(req).await; // API has its own key check
    }
    let s = st.db.settings().unwrap_or_default();
    if s.auth_password.is_empty() || basic_ok(req.headers(), &s) {
        next.run(req).await
    } else {
        (StatusCode::UNAUTHORIZED, [(header::WWW_AUTHENTICATE, "Basic realm=\"Converterr\"")], "Unauthorized")
            .into_response()
    }
}

mod base64_lite {
    pub fn decode(s: &str) -> Option<String> {
        const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut buf = 0u32;
        let mut bits = 0;
        let mut out = Vec::new();
        for c in s.bytes().filter(|c| *c != b'=') {
            let v = T.iter().position(|t| *t == c)? as u32;
            buf = (buf << 6) | v;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push((buf >> bits) as u8);
                buf &= (1 << bits) - 1;
            }
        }
        String::from_utf8(out).ok()
    }
}

// ---------------------------------------------------------------------------
// System / settings
// ---------------------------------------------------------------------------

async fn status(State(st): State<AppState>) -> ApiResult<Json<Value>> {
    let s = st.db.settings()?;
    let ffmpeg = st.ffmpeg_version.lock().unwrap_or_else(|e| e.into_inner()).clone();
    Ok(Json(json!({
        "appName": "Converterr",
        "version": VERSION,
        "ffmpeg": ffmpeg,
        "uptimeSeconds": st.started.elapsed().as_secs(),
        "running": worker::should_run(&s),
        "paused": s.paused,
        "scheduleEnabled": s.schedule_enabled,
        "history": st.db.history_stats()?,
        "queueSize": st.db.queue()?.len(),
        "dataDir": std::env::var("CONVERTERR_DATA").unwrap_or_else(|_| "/config".into()),
    })))
}

async fn get_settings(State(st): State<AppState>) -> ApiResult<Json<Settings>> {
    Ok(Json(st.db.settings()?))
}

async fn put_settings(State(st): State<AppState>, Json(mut s): Json<Settings>) -> ApiResult<Json<Settings>> {
    s.workers = s.workers.clamp(0, worker::MAX_WORKERS as i64);
    if s.api_key.trim().is_empty() {
        s.api_key = crate::db::random_key();
    }
    st.db.save_settings(&s)?;
    st.wake.notify_waiters();
    st.notify_queue();
    st.log("info", "Settings saved");
    Ok(Json(s))
}

#[derive(Deserialize)]
struct ArrTest {
    url: String,
    #[serde(rename = "apiKey")]
    api_key: String,
}

async fn test_arr(Json(t): Json<ArrTest>) -> ApiResult<Json<Value>> {
    let v = arr::test(&t.url, &t.api_key).await?;
    Ok(Json(json!({ "message": v })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TelegramTest {
    bot_token: String,
    chat_id: String,
    #[serde(default)]
    topic_id: String,
}

async fn test_telegram(Json(t): Json<TelegramTest>) -> ApiResult<Json<Value>> {
    notify::send(&t.bot_token, &t.chat_id, &t.topic_id, false, "Converterr - Test", "This is a test message from Converterr").await?;
    Ok(Json(json!({ "message": "sent" })))
}

async fn logs(State(st): State<AppState>) -> Json<Value> {
    let logs = st.logs.lock().unwrap_or_else(|e| e.into_inner());
    Json(json!(logs.iter().rev().collect::<Vec<_>>()))
}

// ---------------------------------------------------------------------------
// Profiles
// ---------------------------------------------------------------------------

async fn list_profiles(State(st): State<AppState>) -> ApiResult<Json<Vec<Profile>>> {
    Ok(Json(st.db.profiles()?))
}

fn validate_profile(p: &Profile) -> ApiResult<()> {
    if p.name.trim().is_empty() {
        return Err(ApiError(StatusCode::BAD_REQUEST, "name is required".into()));
    }
    if p.encoder.trim().is_empty() {
        return Err(ApiError(StatusCode::BAD_REQUEST, "encoder is required".into()));
    }
    Ok(())
}

async fn create_profile(State(st): State<AppState>, Json(mut p): Json<Profile>) -> ApiResult<Json<Profile>> {
    validate_profile(&p)?;
    p.id = 0;
    p.id = st.db.save_profile(&p)?;
    Ok(Json(p))
}

async fn update_profile(State(st): State<AppState>, Path(id): Path<i64>, Json(mut p): Json<Profile>) -> ApiResult<Json<Profile>> {
    validate_profile(&p)?;
    p.id = id;
    st.db.save_profile(&p)?;
    Ok(Json(p))
}

async fn delete_profile(State(st): State<AppState>, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    st.db.delete_profile(id)?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Libraries
// ---------------------------------------------------------------------------

async fn list_libraries(State(st): State<AppState>) -> ApiResult<Json<Value>> {
    let scanning = st.scanning.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let mut out = Vec::new();
    for l in st.db.libraries()? {
        let stats = st.db.library_stats(l.id)?;
        let mut v = serde_json::to_value(&l)?;
        v["stats"] = serde_json::to_value(stats)?;
        v["scanning"] = json!(scanning.contains(&l.id));
        v["exists"] = json!(FsPath::new(&l.path).is_dir());
        out.push(v);
    }
    Ok(Json(json!(out)))
}

fn validate_library(st: &AppState, l: &Library) -> ApiResult<()> {
    if l.name.trim().is_empty() || l.path.trim().is_empty() {
        return Err(ApiError(StatusCode::BAD_REQUEST, "name and path are required".into()));
    }
    if !FsPath::new(&l.path).is_dir() {
        return Err(ApiError(StatusCode::BAD_REQUEST, format!("folder {} does not exist", l.path)));
    }
    if st.db.profile(l.profile_id)?.is_none() {
        return Err(ApiError(StatusCode::BAD_REQUEST, "profile does not exist".into()));
    }
    Ok(())
}

async fn create_library(State(st): State<AppState>, Json(mut l): Json<Library>) -> ApiResult<Json<Library>> {
    validate_library(&st, &l)?;
    l.id = 0;
    l.id = st.db.save_library(&l)?;
    st.log("info", &format!("Library '{}' added", l.name));
    Ok(Json(l))
}

async fn update_library(State(st): State<AppState>, Path(id): Path<i64>, Json(mut l): Json<Library>) -> ApiResult<Json<Library>> {
    validate_library(&st, &l)?;
    l.id = id;
    st.db.save_library(&l)?;
    Ok(Json(l))
}

async fn delete_library(State(st): State<AppState>, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    st.db.delete_library(id)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn scan_one(State(st): State<AppState>, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    st.db.library(id)?.ok_or_else(|| not_found("library"))?;
    tokio::spawn(scanner::scan_library(st.clone(), id));
    Ok(Json(json!({ "message": "scan started" })))
}

async fn scan_all(State(st): State<AppState>) -> ApiResult<Json<Value>> {
    for l in st.db.libraries()? {
        if l.enabled {
            tokio::spawn(scanner::scan_library(st.clone(), l.id));
        }
    }
    Ok(Json(json!({ "message": "scan started" })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FilesQuery {
    #[serde(default)]
    to_convert: bool,
    #[serde(default)]
    search: String,
    #[serde(default = "one")]
    page: i64,
    #[serde(default = "fifty")]
    page_size: i64,
}

fn one() -> i64 {
    1
}
fn fifty() -> i64 {
    50
}

async fn library_files(State(st): State<AppState>, Path(id): Path<i64>, Query(q): Query<FilesQuery>) -> ApiResult<Json<Value>> {
    let size = q.page_size.clamp(1, 500);
    let (records, total) = st.db.files(id, q.to_convert, &q.search, size, (q.page.max(1) - 1) * size)?;
    Ok(Json(json!({ "page": q.page, "pageSize": size, "totalRecords": total, "records": records })))
}

// ---------------------------------------------------------------------------
// Queue / history
// ---------------------------------------------------------------------------

async fn queue(State(st): State<AppState>) -> ApiResult<Json<Value>> {
    let tasks = st.db.queue()?;
    let suspended: Vec<i64> = st
        .workers
        .iter()
        .filter(|w| w.suspended.load(std::sync::atomic::Ordering::SeqCst))
        .map(|w| w.task_id.load(std::sync::atomic::Ordering::SeqCst))
        .collect();
    let mut out = Vec::new();
    for t in tasks {
        let mut v = serde_json::to_value(&t)?;
        v["suspended"] = json!(suspended.contains(&t.id));
        out.push(v);
    }
    Ok(Json(json!(out)))
}

#[derive(Deserialize)]
struct QueueAdd {
    /// A file, a folder, or several of them.
    #[serde(default)]
    path: String,
    #[serde(default)]
    paths: Vec<String>,
    /// Start right away, ignoring the time window.
    #[serde(default)]
    force: bool,
}

/// Find the library a path belongs to (longest matching root wins).
fn library_for(st: &AppState, path: &str) -> ApiResult<Option<Library>> {
    Ok(st
        .db
        .libraries()?
        .into_iter()
        .filter(|l| path.starts_with(&format!("{}/", l.path.trim_end_matches('/'))))
        .max_by_key(|l| l.path.len()))
}

/// Queues files (or every matching file in folders) regardless of the profile
/// rules; with `force` they start right away, ignoring the time window.
async fn queue_add(State(st): State<AppState>, Json(q): Json<QueueAdd>) -> ApiResult<Json<Value>> {
    let mut inputs = q.paths.clone();
    if !q.path.trim().is_empty() {
        inputs.push(q.path.clone());
    }
    if inputs.is_empty() {
        return Err(ApiError(StatusCode::BAD_REQUEST, "path is required".into()));
    }
    let (mut queued, mut forced) = (0, 0);
    for input in inputs {
        let path = PathBuf::from(input.trim());
        let lib = library_for(&st, &path.to_string_lossy())?.ok_or_else(|| {
            ApiError(StatusCode::BAD_REQUEST, format!("{} is not inside any library", path.display()))
        })?;
        let files: Vec<PathBuf> = if path.is_dir() {
            walkdir::WalkDir::new(&path)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file() && lib.accepts(e.path()))
                .map(|e| e.into_path())
                .collect()
        } else {
            vec![path]
        };
        for f in files {
            let key = f.to_string_lossy().into_owned();
            let size = std::fs::metadata(&f).map(|m| m.len() as i64).unwrap_or(0);
            if st.db.enqueue(lib.id, &key, size, "", 10)?.is_some() {
                queued += 1;
            }
            if q.force && st.db.force_path(&key)? {
                forced += 1;
            }
        }
    }
    st.notify_queue();
    st.log(
        "info",
        &format!("{queued} file(s) queued manually{}", if q.force { format!(", {forced} forced") } else { String::new() }),
    );
    Ok(Json(json!({ "queued": queued, "forced": forced })))
}

#[derive(Deserialize)]
struct ForceReq {
    ids: Vec<i64>,
}

/// Start queued tasks right away, ignoring the time window.
async fn queue_force(State(st): State<AppState>, Json(r): Json<ForceReq>) -> ApiResult<Json<Value>> {
    let mut forced = 0;
    for id in r.ids {
        if st.db.force_task(id)? {
            forced += 1;
        }
    }
    st.notify_queue();
    st.log("info", &format!("{forced} task(s) forced"));
    Ok(Json(json!({ "forced": forced })))
}

async fn queue_remove(State(st): State<AppState>, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    let t = st.db.task(id)?.ok_or_else(|| not_found("task"))?;
    if t.status == "running" {
        worker::cancel_task(&st, id);
    } else {
        st.db.set_task_status(id, "cancelled")?;
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn queue_top(State(st): State<AppState>, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    st.db.set_task_priority(id, 100)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryQuery {
    #[serde(default)]
    status: String,
    #[serde(default = "one")]
    page: i64,
    #[serde(default = "fifty")]
    page_size: i64,
}

async fn history(State(st): State<AppState>, Query(q): Query<HistoryQuery>) -> ApiResult<Json<Value>> {
    let size = q.page_size.clamp(1, 500);
    let (records, total) = st.db.history(&q.status, size, (q.page.max(1) - 1) * size)?;
    Ok(Json(json!({
        "page": q.page, "pageSize": size, "totalRecords": total, "records": records,
        "stats": st.db.history_stats()?,
    })))
}

async fn clear_history(State(st): State<AppState>) -> ApiResult<Json<Value>> {
    let n = st.db.clear_history()?;
    Ok(Json(json!({ "deleted": n })))
}

async fn retry(State(st): State<AppState>, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    st.db.task(id)?.ok_or_else(|| not_found("task"))?;
    st.db.requeue_task(id)?;
    st.notify_queue();
    Ok(StatusCode::NO_CONTENT)
}

async fn task_log(State(st): State<AppState>, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    Ok(Json(json!({ "log": st.db.task_log(id)? })))
}

// ---------------------------------------------------------------------------
// Workers
// ---------------------------------------------------------------------------

async fn workers(State(st): State<AppState>) -> ApiResult<Json<Value>> {
    let s = st.db.settings()?;
    Ok(Json(json!({
        "running": worker::should_run(&s),
        "paused": s.paused,
        "workers": worker::workers_info(&st),
    })))
}

async fn set_paused(st: &AppState, paused: bool) -> ApiResult<StatusCode> {
    let mut s = st.db.settings()?;
    s.paused = paused;
    st.db.save_settings(&s)?;
    st.wake.notify_waiters();
    st.notify_queue();
    Ok(StatusCode::NO_CONTENT)
}

async fn pause(State(st): State<AppState>) -> ApiResult<StatusCode> {
    set_paused(&st, true).await
}

async fn resume(State(st): State<AppState>) -> ApiResult<StatusCode> {
    set_paused(&st, false).await
}

// ---------------------------------------------------------------------------
// Folder browser (for the "add library" dialog)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct FsQuery {
    #[serde(default)]
    path: String,
}

async fn filesystem(Query(q): Query<FsQuery>) -> ApiResult<Json<Value>> {
    let path = if q.path.is_empty() { "/".to_string() } else { q.path };
    let mut dirs = Vec::new();
    let mut rd = tokio::fs::read_dir(&path).await?;
    while let Some(e) = rd.next_entry().await? {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        if e.file_type().await.map(|t| t.is_dir()).unwrap_or(false)
            || tokio::fs::metadata(e.path()).await.map(|m| m.is_dir()).unwrap_or(false)
        {
            dirs.push(json!({ "name": name, "path": e.path().to_string_lossy() }));
        }
    }
    dirs.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    let parent = FsPath::new(&path).parent().map(|p| p.to_string_lossy().into_owned());
    Ok(Json(json!({ "path": path, "parent": parent, "directories": dirs })))
}

// ---------------------------------------------------------------------------
// Sonarr / Radarr webhook: Settings > Connect > Webhook, On Import,
// URL http://converterr:8787/api/v1/webhook?apikey=<key>
// ---------------------------------------------------------------------------

async fn webhook(State(st): State<AppState>, Json(payload): Json<Value>) -> ApiResult<Json<Value>> {
    let event = payload.get("eventType").and_then(|e| e.as_str()).unwrap_or("?").to_string();
    let mut queued = 0;
    for remote in arr::webhook_paths(&payload) {
        let mut matched = false;
        for lib in st.db.libraries()?.into_iter().filter(|l| l.enabled) {
            let Some(local) = arr::from_arr_path(&lib, &remote) else { continue };
            matched = true;
            let Some(profile) = st.db.profile(lib.profile_id)? else { continue };
            match scanner::check_file(&st, &lib, &profile, FsPath::new(&local), false).await {
                Ok(Some(f)) => {
                    queued += 1;
                    notify::event(
                        &st,
                        notify::Event::Queued,
                        "File Queued",
                        format!(
                            "{}\n{}\nLibrary: {} (from {})",
                            notify::html(&notify::file_name(FsPath::new(&f.path))),
                            notify::html(&scanner::describe(&f)),
                            notify::html(&lib.name),
                            notify::html(payload.pointer("/instanceName").and_then(|v| v.as_str()).unwrap_or("Sonarr/Radarr"))
                        ),
                    );
                }
                Ok(None) => {}
                Err(e) => st.log("warn", &format!("webhook: {local}: {e:#}")),
            }
            break;
        }
        if !matched {
            st.log("warn", &format!("webhook: {remote} is not inside any library (check the library's Sonarr/Radarr path)"));
        }
    }
    st.log("info", &format!("Webhook '{event}' received, {queued} file(s) queued"));
    Ok(Json(json!({ "queued": queued })))
}
