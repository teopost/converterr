//! Converterr — automatic media library transcoder with an *arr-style UI.
//!
//! Environment:
//!   CONVERTERR_DATA  data directory (database), default /config
//!   CONVERTERR_PORT  HTTP port, default 8787
//!   CONVERTERR_BIND  bind address, default 0.0.0.0
//!   RUST_LOG         log filter, default info

mod api;
mod arr;
mod db;
mod ffmpeg;
mod notify;
mod probe;
mod scanner;
mod state;
mod worker;

use std::path::PathBuf;

use anyhow::{Context, Result};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let data = PathBuf::from(std::env::var("CONVERTERR_DATA").unwrap_or_else(|_| "/config".into()));
    std::fs::create_dir_all(&data).with_context(|| format!("creating {}", data.display()))?;
    let db = db::Db::open(&data.join("converterr.db"))?;

    let state = state::AppState::new(db);
    let ffmpeg_version = ffmpeg::version().await;
    *state.ffmpeg_version.lock().unwrap() = ffmpeg_version.clone();
    state.log("info", &format!("Converterr {} starting ({ffmpeg_version})", api::VERSION));

    let cache = state.db.settings()?.cache_dir;
    if let Err(e) = std::fs::create_dir_all(&cache) {
        state.log("warn", &format!("cache dir {cache}: {e}"));
    }

    worker::start(state.clone());
    scanner::start(state.clone());

    let port = std::env::var("CONVERTERR_PORT").unwrap_or_else(|_| "8787".into());
    let bind = std::env::var("CONVERTERR_BIND").unwrap_or_else(|_| "0.0.0.0".into());
    let addr = format!("{bind}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await.with_context(|| format!("binding {addr}"))?;
    state.log("info", &format!("Web UI listening on http://{addr}"));

    axum::serve(listener, api::router(state))
        .with_graceful_shutdown(async {
            let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = term.recv() => {}
            }
        })
        .await?;
    Ok(())
}
