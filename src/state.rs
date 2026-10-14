//! Shared application state and the in-memory log buffer shown in the UI.

use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde::Serialize;
use tokio::sync::Notify;

use crate::db::Db;
use crate::worker::{WorkerSlot, MAX_WORKERS};

#[derive(Clone, Serialize)]
pub struct LogLine {
    pub time: String,
    pub level: String,
    pub message: String,
}

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    /// Regular worker slots 0..MAX_WORKERS, plus one slot for forced tasks.
    pub workers: Arc<Vec<Arc<WorkerSlot>>>,
    /// Woken when tasks are queued so idle workers start immediately.
    pub queue_changed: Arc<Notify>,
    /// Woken when settings change so the supervisor reacts immediately.
    pub wake: Arc<Notify>,
    pub logs: Arc<Mutex<VecDeque<LogLine>>>,
    /// Library ids currently being scanned.
    pub scanning: Arc<Mutex<HashSet<i64>>>,
    pub started: Instant,
    pub ffmpeg_version: Arc<Mutex<String>>,
}

impl AppState {
    pub fn new(db: Db) -> Self {
        AppState {
            db,
            workers: Arc::new((0..=MAX_WORKERS).map(|_| Arc::new(WorkerSlot::default())).collect()),
            queue_changed: Arc::new(Notify::new()),
            wake: Arc::new(Notify::new()),
            logs: Arc::new(Mutex::new(VecDeque::new())),
            scanning: Arc::new(Mutex::new(HashSet::new())),
            started: Instant::now(),
            ffmpeg_version: Arc::new(Mutex::new(String::new())),
        }
    }

    pub fn log(&self, level: &str, message: &str) {
        match level {
            "error" => tracing::error!("{message}"),
            "warn" => tracing::warn!("{message}"),
            _ => tracing::info!("{message}"),
        }
        let mut logs = self.logs.lock().unwrap_or_else(|e| e.into_inner());
        if logs.len() >= 1000 {
            logs.pop_front();
        }
        logs.push_back(LogLine {
            time: crate::db::now(),
            level: level.into(),
            message: message.into(),
        });
    }

    pub fn notify_queue(&self) {
        self.queue_changed.notify_waiters();
    }
}
