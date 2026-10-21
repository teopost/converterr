//! SQLite persistence: schema, models and queries.

use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

// ---------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub api_key: String,
    /// Number of concurrent conversion workers.
    pub workers: i64,
    /// Global pause switch (survives restarts).
    pub paused: bool,
    /// Only run workers inside the [schedule_start, schedule_end) window.
    pub schedule_enabled: bool,
    pub schedule_start: String,
    pub schedule_end: String,
    /// Where ffmpeg writes its output before it replaces the original.
    pub cache_dir: String,
    /// Automatic library scan interval, 0 = disabled.
    pub scan_interval_minutes: i64,
    /// Max parallel ffprobe calls while scanning.
    pub probe_concurrency: i64,
    pub sonarr_url: String,
    pub sonarr_api_key: String,
    pub radarr_url: String,
    pub radarr_api_key: String,
    /// Web UI login: none | basic (browser popup) | forms (login page).
    pub auth_method: String,
    /// enabled | disabledForLocalAddresses (no login from LAN/loopback).
    pub auth_required: String,
    pub auth_username: String,
    pub auth_password: String,
    /// Telegram notifications (same bot/chat as Sonarr/Radarr can be reused).
    pub telegram_bot_token: String,
    pub telegram_chat_id: String,
    pub telegram_topic_id: String,
    pub telegram_silent: bool,
    pub telegram_on_queued: bool,
    pub telegram_on_completed: bool,
    pub telegram_on_failed: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            api_key: String::new(),
            workers: 1,
            paused: false,
            schedule_enabled: false,
            schedule_start: "01:00".into(),
            schedule_end: "08:00".into(),
            cache_dir: "/cache".into(),
            scan_interval_minutes: 0,
            probe_concurrency: 2,
            sonarr_url: String::new(),
            sonarr_api_key: String::new(),
            radarr_url: String::new(),
            radarr_api_key: String::new(),
            // Empty = settings saved before auth_method existed, see `Db::seed`.
            auth_method: String::new(),
            auth_required: "enabled".into(),
            auth_username: "admin".into(),
            auth_password: String::new(),
            telegram_bot_token: String::new(),
            telegram_chat_id: String::new(),
            telegram_topic_id: String::new(),
            telegram_silent: false,
            telegram_on_queued: true,
            telegram_on_completed: true,
            telegram_on_failed: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Profile {
    pub id: i64,
    pub name: String,
    /// Comma separated source video codecs (ffprobe names) to convert.
    /// Empty = every codec different from the target one.
    pub source_codecs: String,
    /// Also convert files larger than max_width x max_height.
    pub convert_oversized: bool,
    pub max_width: i64,
    pub max_height: i64,
    /// ffmpeg video encoder: libx265, libx264, libsvtav1, ...
    pub encoder: String,
    pub preset: String,
    pub crf: i64,
    /// Output pixel format, empty = keep source.
    pub pix_fmt: String,
    /// mkv | mp4 | keep
    pub container: String,
    /// copy | aac | ac3
    pub audio: String,
    pub extra_args: String,
    /// Keep the original if the converted file is not smaller.
    pub skip_if_larger: bool,
}

impl Default for Profile {
    fn default() -> Self {
        Profile {
            id: 0,
            name: String::new(),
            source_codecs: String::new(),
            convert_oversized: false,
            max_width: 1920,
            max_height: 1080,
            encoder: "libx265".into(),
            preset: "veryfast".into(),
            crf: 26,
            pix_fmt: "yuv420p".into(),
            container: "mkv".into(),
            audio: "copy".into(),
            extra_args: String::new(),
            skip_if_larger: true,
        }
    }
}

impl Profile {
    /// ffprobe codec name produced by the configured encoder.
    pub fn target_codec(&self) -> &'static str {
        match self.encoder.as_str() {
            e if e.contains("265") || e.contains("hevc") => "hevc",
            e if e.contains("264") => "h264",
            e if e.contains("av1") => "av1",
            e if e.contains("vp9") => "vp9",
            _ => "",
        }
    }

    pub fn source_codec_list(&self) -> Vec<String> {
        self.source_codecs
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Library {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub profile_id: i64,
    pub enabled: bool,
    /// Comma separated list of file extensions to consider.
    pub extensions: String,
    /// none | sonarr | radarr
    pub notify: String,
    /// The same folder as seen by Sonarr/Radarr (for webhooks and rescans).
    pub arr_path: String,
}

impl Default for Library {
    fn default() -> Self {
        Library {
            id: 0,
            name: String::new(),
            path: String::new(),
            profile_id: 1,
            enabled: true,
            extensions: "mkv,mp4,avi,m4v,mov,wmv,ts,m2ts,mpg,mpeg".into(),
            notify: "none".into(),
            arr_path: String::new(),
        }
    }
}

impl Library {
    pub fn accepts(&self, path: &Path) -> bool {
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            return false;
        };
        let ext = ext.to_lowercase();
        self.extensions
            .split(',')
            .any(|e| e.trim().trim_start_matches('.').eq_ignore_ascii_case(&ext))
    }
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: i64,
    pub library_id: i64,
    pub path: String,
    /// queued | running | completed | skipped | failed | cancelled
    pub status: String,
    pub priority: i64,
    pub progress: f64,
    pub speed: String,
    pub eta_seconds: Option<i64>,
    pub size_before: i64,
    pub size_after: Option<i64>,
    pub codec_before: String,
    pub codec_after: String,
    pub output_path: String,
    pub error: String,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    /// Forced tasks run right away, ignoring the time window.
    pub force: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub path: String,
    pub library_id: i64,
    pub size: i64,
    pub mtime: i64,
    pub codec: String,
    pub width: i64,
    pub height: i64,
    pub bit_depth: i64,
    pub duration: f64,
    pub needs_convert: bool,
    pub reason: String,
    pub checked_at: String,
}

// ---------------------------------------------------------------------------
// Database handle
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct Db(Arc<Mutex<Connection>>);

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS config (
    id   INTEGER PRIMARY KEY CHECK (id = 1),
    json TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS profiles (
    id   INTEGER PRIMARY KEY AUTOINCREMENT,
    json TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS libraries (
    id   INTEGER PRIMARY KEY AUTOINCREMENT,
    json TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS tasks (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    library_id    INTEGER NOT NULL,
    path          TEXT NOT NULL,
    status        TEXT NOT NULL,
    priority      INTEGER NOT NULL DEFAULT 0,
    progress      REAL NOT NULL DEFAULT 0,
    speed         TEXT NOT NULL DEFAULT '',
    eta_seconds   INTEGER,
    size_before   INTEGER NOT NULL DEFAULT 0,
    size_after    INTEGER,
    codec_before  TEXT NOT NULL DEFAULT '',
    codec_after   TEXT NOT NULL DEFAULT '',
    output_path   TEXT NOT NULL DEFAULT '',
    error         TEXT NOT NULL DEFAULT '',
    log           TEXT NOT NULL DEFAULT '',
    created_at    TEXT NOT NULL,
    started_at    TEXT,
    finished_at   TEXT
);
CREATE INDEX IF NOT EXISTS tasks_status ON tasks(status);
CREATE INDEX IF NOT EXISTS tasks_path ON tasks(path);
CREATE TABLE IF NOT EXISTS files (
    path          TEXT PRIMARY KEY,
    library_id    INTEGER NOT NULL,
    size          INTEGER NOT NULL,
    mtime         INTEGER NOT NULL,
    codec         TEXT NOT NULL DEFAULT '',
    width         INTEGER NOT NULL DEFAULT 0,
    height        INTEGER NOT NULL DEFAULT 0,
    bit_depth     INTEGER NOT NULL DEFAULT 0,
    duration      REAL NOT NULL DEFAULT 0,
    needs_convert INTEGER NOT NULL DEFAULT 0,
    reason        TEXT NOT NULL DEFAULT '',
    checked_at    TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS files_library ON files(library_id);
CREATE TABLE IF NOT EXISTS sessions (
    token      TEXT PRIMARY KEY,
    created_at INTEGER NOT NULL
);
"#;

const TASK_COLS: &str = "id, library_id, path, status, priority, progress, speed, eta_seconds, \
     size_before, size_after, codec_before, codec_after, output_path, error, created_at, \
     started_at, finished_at, force";

fn task_from_row(r: &Row) -> rusqlite::Result<Task> {
    Ok(Task {
        id: r.get(0)?,
        library_id: r.get(1)?,
        path: r.get(2)?,
        status: r.get(3)?,
        priority: r.get(4)?,
        progress: r.get(5)?,
        speed: r.get(6)?,
        eta_seconds: r.get(7)?,
        size_before: r.get(8)?,
        size_after: r.get(9)?,
        codec_before: r.get(10)?,
        codec_after: r.get(11)?,
        output_path: r.get(12)?,
        error: r.get(13)?,
        created_at: r.get(14)?,
        started_at: r.get(15)?,
        finished_at: r.get(16)?,
        force: r.get(17)?,
    })
}

const FILE_COLS: &str = "path, library_id, size, mtime, codec, width, height, bit_depth, \
     duration, needs_convert, reason, checked_at";

fn file_from_row(r: &Row) -> rusqlite::Result<FileEntry> {
    Ok(FileEntry {
        path: r.get(0)?,
        library_id: r.get(1)?,
        size: r.get(2)?,
        mtime: r.get(3)?,
        codec: r.get(4)?,
        width: r.get(5)?,
        height: r.get(6)?,
        bit_depth: r.get(7)?,
        duration: r.get(8)?,
        needs_convert: r.get(9)?,
        reason: r.get(10)?,
        checked_at: r.get(11)?,
    })
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LibraryStats {
    pub files: i64,
    pub size: i64,
    pub to_convert: i64,
    pub to_convert_size: i64,
    pub codecs: Vec<(String, i64)>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HistoryStats {
    pub completed: i64,
    pub failed: i64,
    pub saved_bytes: i64,
}

impl Db {
    pub fn open(path: &Path) -> Result<Db> {
        let conn = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "busy_timeout", 5000)?;
        conn.execute_batch(SCHEMA)?;
        // Migrations for databases created by older versions.
        let has_force: bool = conn
            .prepare("SELECT 1 FROM pragma_table_info('tasks') WHERE name='force'")?
            .exists([])?;
        if !has_force {
            conn.execute_batch("ALTER TABLE tasks ADD COLUMN force INTEGER NOT NULL DEFAULT 0")?;
        }
        let db = Db(Arc::new(Mutex::new(conn)));
        db.seed()?;
        Ok(db)
    }

    fn conn(&self) -> MutexGuard<'_, Connection> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn seed(&self) -> Result<()> {
        let mut s = self.settings()?;
        if s.api_key.is_empty() {
            s.api_key = random_key();
            self.save_settings(&s)?;
        }
        if s.auth_method.is_empty() {
            // Older versions: a password meant Basic auth, no password no login.
            s.auth_method = if s.auth_password.is_empty() { "none" } else { "basic" }.into();
            self.save_settings(&s)?;
        }
        if self.profiles()?.is_empty() {
            self.save_profile(&Profile {
                name: "TV compatible (HEVC 720p)".into(),
                source_codecs: "av1,vc1,wmv3,wmv2,mpeg2video,msmpeg4v3".into(),
                max_width: 1280,
                max_height: 720,
                ..Profile::default()
            })?;
        }
        // Anything that was running when we stopped goes back to the queue.
        self.conn().execute(
            "UPDATE tasks SET status='queued', progress=0, speed='', eta_seconds=NULL, started_at=NULL \
             WHERE status='running'",
            [],
        )?;
        Ok(())
    }

    // -- settings ----------------------------------------------------------

    pub fn settings(&self) -> Result<Settings> {
        let json: Option<String> = self
            .conn()
            .query_row("SELECT json FROM config WHERE id=1", [], |r| r.get(0))
            .optional()?;
        Ok(match json {
            Some(j) => serde_json::from_str(&j).unwrap_or_default(),
            None => Settings::default(),
        })
    }

    pub fn save_settings(&self, s: &Settings) -> Result<()> {
        self.conn().execute(
            "INSERT INTO config (id, json) VALUES (1, ?1) ON CONFLICT(id) DO UPDATE SET json=excluded.json",
            params![serde_json::to_string(s)?],
        )?;
        Ok(())
    }

    // -- login sessions (forms auth) ---------------------------------------

    /// New session token; also drops sessions older than `max_age_secs`.
    pub fn session_create(&self, max_age_secs: i64) -> Result<String> {
        let token = format!("{}{}", random_key(), random_key());
        let now = chrono::Utc::now().timestamp();
        let conn = self.conn();
        conn.execute("DELETE FROM sessions WHERE created_at<?1", params![now - max_age_secs])?;
        conn.execute("INSERT INTO sessions (token, created_at) VALUES (?1, ?2)", params![token, now])?;
        Ok(token)
    }

    /// Valid if it exists and is younger than `max_age_secs`.
    pub fn session_valid(&self, token: &str, max_age_secs: i64) -> bool {
        let oldest = chrono::Utc::now().timestamp() - max_age_secs;
        self.conn()
            .query_row("SELECT 1 FROM sessions WHERE token=?1 AND created_at>=?2", params![token, oldest], |_| Ok(()))
            .is_ok()
    }

    pub fn session_delete(&self, token: &str) -> Result<()> {
        self.conn().execute("DELETE FROM sessions WHERE token=?1", params![token])?;
        Ok(())
    }

    /// Logs everybody out (credentials or auth method changed).
    pub fn sessions_clear(&self) -> Result<()> {
        self.conn().execute("DELETE FROM sessions", [])?;
        Ok(())
    }

    // -- generic json-blob tables (profiles, libraries) ---------------------

    fn list_json<T: for<'de> Deserialize<'de>>(&self, table: &str, set_id: impl Fn(&mut T, i64)) -> Result<Vec<T>> {
        let conn = self.conn();
        let mut st = conn.prepare(&format!("SELECT id, json FROM {table} ORDER BY id"))?;
        let rows = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
        let mut out = Vec::new();
        for row in rows {
            let (id, json) = row?;
            if let Ok(mut v) = serde_json::from_str::<T>(&json) {
                set_id(&mut v, id);
                out.push(v);
            }
        }
        Ok(out)
    }

    fn save_json<T: Serialize>(&self, table: &str, id: i64, v: &T) -> Result<i64> {
        let json = serde_json::to_string(v)?;
        let conn = self.conn();
        if id > 0 {
            let n = conn.execute(&format!("UPDATE {table} SET json=?1 WHERE id=?2"), params![json, id])?;
            anyhow::ensure!(n == 1, "not found");
            Ok(id)
        } else {
            conn.execute(&format!("INSERT INTO {table} (json) VALUES (?1)"), params![json])?;
            Ok(conn.last_insert_rowid())
        }
    }

    pub fn profiles(&self) -> Result<Vec<Profile>> {
        self.list_json("profiles", |p: &mut Profile, id| p.id = id)
    }

    pub fn profile(&self, id: i64) -> Result<Option<Profile>> {
        Ok(self.profiles()?.into_iter().find(|p| p.id == id))
    }

    pub fn save_profile(&self, p: &Profile) -> Result<i64> {
        self.save_json("profiles", p.id, p)
    }

    pub fn delete_profile(&self, id: i64) -> Result<()> {
        anyhow::ensure!(
            !self.libraries()?.iter().any(|l| l.profile_id == id),
            "profile is used by a library"
        );
        self.conn().execute("DELETE FROM profiles WHERE id=?1", params![id])?;
        Ok(())
    }

    pub fn libraries(&self) -> Result<Vec<Library>> {
        self.list_json("libraries", |l: &mut Library, id| l.id = id)
    }

    pub fn library(&self, id: i64) -> Result<Option<Library>> {
        Ok(self.libraries()?.into_iter().find(|l| l.id == id))
    }

    pub fn save_library(&self, l: &Library) -> Result<i64> {
        self.save_json("libraries", l.id, l)
    }

    pub fn delete_library(&self, id: i64) -> Result<()> {
        let conn = self.conn();
        conn.execute("DELETE FROM libraries WHERE id=?1", params![id])?;
        conn.execute("DELETE FROM files WHERE library_id=?1", params![id])?;
        conn.execute("DELETE FROM tasks WHERE library_id=?1 AND status='queued'", params![id])?;
        Ok(())
    }

    // -- files -------------------------------------------------------------

    pub fn file(&self, path: &str) -> Result<Option<FileEntry>> {
        Ok(self
            .conn()
            .query_row(&format!("SELECT {FILE_COLS} FROM files WHERE path=?1"), params![path], file_from_row)
            .optional()?)
    }

    pub fn upsert_file(&self, f: &FileEntry) -> Result<()> {
        self.conn().execute(
            &format!(
                "INSERT INTO files ({FILE_COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12) \
                 ON CONFLICT(path) DO UPDATE SET library_id=excluded.library_id, size=excluded.size, \
                 mtime=excluded.mtime, codec=excluded.codec, width=excluded.width, height=excluded.height, \
                 bit_depth=excluded.bit_depth, duration=excluded.duration, needs_convert=excluded.needs_convert, \
                 reason=excluded.reason, checked_at=excluded.checked_at"
            ),
            params![
                f.path,
                f.library_id,
                f.size,
                f.mtime,
                f.codec,
                f.width,
                f.height,
                f.bit_depth,
                f.duration,
                f.needs_convert,
                f.reason,
                f.checked_at
            ],
        )?;
        Ok(())
    }

    pub fn delete_file(&self, path: &str) -> Result<()> {
        self.conn().execute("DELETE FROM files WHERE path=?1", params![path])?;
        Ok(())
    }

    pub fn library_file_paths(&self, library_id: i64) -> Result<Vec<String>> {
        let conn = self.conn();
        let mut st = conn.prepare("SELECT path FROM files WHERE library_id=?1")?;
        let rows = st.query_map(params![library_id], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn files(&self, library_id: i64, only_to_convert: bool, search: &str, limit: i64, offset: i64) -> Result<(Vec<FileEntry>, i64)> {
        let conn = self.conn();
        let mut filter = String::from("library_id=?1 AND path LIKE ?2");
        if only_to_convert {
            filter.push_str(" AND needs_convert=1");
        }
        let like = format!("%{}%", search.replace('%', ""));
        let total: i64 =
            conn.query_row(&format!("SELECT COUNT(*) FROM files WHERE {filter}"), params![library_id, like], |r| r.get(0))?;
        let mut st = conn.prepare(&format!(
            "SELECT {FILE_COLS} FROM files WHERE {filter} ORDER BY path LIMIT ?3 OFFSET ?4"
        ))?;
        let rows = st.query_map(params![library_id, like, limit, offset], file_from_row)?;
        Ok((rows.collect::<rusqlite::Result<_>>()?, total))
    }

    pub fn library_stats(&self, library_id: i64) -> Result<LibraryStats> {
        let conn = self.conn();
        let (files, size, to_convert, to_convert_size) = conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(size),0), COALESCE(SUM(needs_convert),0), \
             COALESCE(SUM(CASE WHEN needs_convert=1 THEN size ELSE 0 END),0) FROM files WHERE library_id=?1",
            params![library_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;
        let mut st = conn.prepare(
            "SELECT codec, COUNT(*) FROM files WHERE library_id=?1 GROUP BY codec ORDER BY COUNT(*) DESC",
        )?;
        let codecs = st
            .query_map(params![library_id], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(LibraryStats { files, size, to_convert, to_convert_size, codecs })
    }

    // -- tasks -------------------------------------------------------------

    /// Queue a file unless it is already queued/running. Returns the task id.
    pub fn enqueue(&self, library_id: i64, path: &str, size: i64, codec: &str, priority: i64) -> Result<Option<i64>> {
        let conn = self.conn();
        let existing: Option<i64> = conn
            .query_row(
                "SELECT id FROM tasks WHERE path=?1 AND status IN ('queued','running')",
                params![path],
                |r| r.get(0),
            )
            .optional()?;
        if existing.is_some() {
            return Ok(None);
        }
        conn.execute(
            "INSERT INTO tasks (library_id, path, status, priority, size_before, codec_before, created_at) \
             VALUES (?1, ?2, 'queued', ?3, ?4, ?5, ?6)",
            params![library_id, path, priority, size, codec, now()],
        )?;
        Ok(Some(conn.last_insert_rowid()))
    }

    /// Whether the file already went through a task that must not be retried automatically.
    pub fn has_finished_task(&self, path: &str) -> Result<bool> {
        Ok(self
            .conn()
            .query_row(
                "SELECT 1 FROM tasks WHERE path=?1 AND status IN ('failed','skipped','cancelled') LIMIT 1",
                params![path],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    /// Marks a queued task as forced (runs now, ignoring the time window).
    pub fn force_task(&self, id: i64) -> Result<bool> {
        Ok(self
            .conn()
            .execute("UPDATE tasks SET force=1 WHERE id=?1 AND status='queued'", params![id])?
            == 1)
    }

    /// Forces the queued task of a file, if any.
    pub fn force_path(&self, path: &str) -> Result<bool> {
        Ok(self
            .conn()
            .execute("UPDATE tasks SET force=1 WHERE path=?1 AND status='queued'", params![path])?
            == 1)
    }

    /// Atomically take the next queued task (only forced ones if `forced_only`).
    pub fn claim_next(&self, forced_only: bool) -> Result<Option<Task>> {
        let conn = self.conn();
        let id: Option<i64> = conn
            .query_row(
                "SELECT id FROM tasks WHERE status='queued' AND (?1=0 OR force=1) \
                 ORDER BY force DESC, priority DESC, id LIMIT 1",
                params![forced_only],
                |r| r.get(0),
            )
            .optional()?;
        let Some(id) = id else { return Ok(None) };
        conn.execute(
            "UPDATE tasks SET status='running', started_at=?2, progress=0, error='' WHERE id=?1",
            params![id, now()],
        )?;
        Ok(conn
            .query_row(&format!("SELECT {TASK_COLS} FROM tasks WHERE id=?1"), params![id], task_from_row)
            .optional()?)
    }

    pub fn task(&self, id: i64) -> Result<Option<Task>> {
        Ok(self
            .conn()
            .query_row(&format!("SELECT {TASK_COLS} FROM tasks WHERE id=?1"), params![id], task_from_row)
            .optional()?)
    }

    pub fn task_log(&self, id: i64) -> Result<String> {
        Ok(self
            .conn()
            .query_row("SELECT log FROM tasks WHERE id=?1", params![id], |r| r.get(0))
            .optional()?
            .unwrap_or_default())
    }

    pub fn task_progress(&self, id: i64, progress: f64, speed: &str, eta: Option<i64>) -> Result<()> {
        self.conn().execute(
            "UPDATE tasks SET progress=?2, speed=?3, eta_seconds=?4 WHERE id=?1",
            params![id, progress, speed, eta],
        )?;
        Ok(())
    }

    pub fn task_set_codec(&self, id: i64, codec: &str, size: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE tasks SET codec_before=?2, size_before=?3 WHERE id=?1",
            params![id, codec, size],
        )?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn task_finish(
        &self,
        id: i64,
        status: &str,
        error: &str,
        size_after: Option<i64>,
        codec_after: &str,
        output_path: &str,
        log: &str,
    ) -> Result<()> {
        self.conn().execute(
            "UPDATE tasks SET status=?2, error=?3, size_after=?4, codec_after=?5, output_path=?6, log=?7, \
             finished_at=?8, progress=CASE WHEN ?2='completed' THEN 100 ELSE progress END, eta_seconds=NULL \
             WHERE id=?1",
            params![id, status, error, size_after, codec_after, output_path, log, now()],
        )?;
        Ok(())
    }

    pub fn requeue_task(&self, id: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE tasks SET status='queued', progress=0, speed='', eta_seconds=NULL, started_at=NULL, \
             finished_at=NULL, error='' WHERE id=?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn set_task_status(&self, id: i64, status: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE tasks SET status=?2, finished_at=?3 WHERE id=?1",
            params![id, status, now()],
        )?;
        Ok(())
    }

    pub fn set_task_priority(&self, id: i64, priority: i64) -> Result<()> {
        self.conn().execute("UPDATE tasks SET priority=?2 WHERE id=?1", params![id, priority])?;
        Ok(())
    }

    pub fn queue(&self) -> Result<Vec<Task>> {
        let conn = self.conn();
        let mut st = conn.prepare(&format!(
            "SELECT {TASK_COLS} FROM tasks WHERE status IN ('queued','running') \
             ORDER BY status='running' DESC, force DESC, priority DESC, id"
        ))?;
        let rows = st.query_map([], task_from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn history(&self, status: &str, limit: i64, offset: i64) -> Result<(Vec<Task>, i64)> {
        let conn = self.conn();
        let filter = match status {
            "" | "all" => "status NOT IN ('queued','running')".to_string(),
            s => format!("status='{}'", s.replace('\'', "")),
        };
        let total: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM tasks WHERE {filter}"), [], |r| r.get(0))?;
        let mut st = conn.prepare(&format!(
            "SELECT {TASK_COLS} FROM tasks WHERE {filter} ORDER BY finished_at DESC, id DESC LIMIT ?1 OFFSET ?2"
        ))?;
        let rows = st.query_map(params![limit, offset], task_from_row)?;
        Ok((rows.collect::<rusqlite::Result<_>>()?, total))
    }

    pub fn clear_history(&self) -> Result<usize> {
        Ok(self
            .conn()
            .execute("DELETE FROM tasks WHERE status NOT IN ('queued','running')", [])?)
    }

    pub fn history_stats(&self) -> Result<HistoryStats> {
        Ok(self.conn().query_row(
            "SELECT COALESCE(SUM(status='completed'),0), COALESCE(SUM(status='failed'),0), \
             COALESCE(SUM(CASE WHEN status='completed' THEN size_before - COALESCE(size_after, size_before) ELSE 0 END),0) \
             FROM tasks",
            [],
            |r| Ok(HistoryStats { completed: r.get(0)?, failed: r.get(1)?, saved_bytes: r.get(2)? }),
        )?)
    }
}

pub fn random_key() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..32).map(|_| format!("{:x}", rng.gen_range(0..16u8))).collect()
}
