//! Telegram notifications, formatted like Sonarr/Radarr's Telegram connection
//! ("<b>App - Event</b>" title followed by the details, HTML parse mode).

use std::path::Path;
use std::time::Duration;

use anyhow::{bail, Result};
use serde_json::json;

use crate::db::Settings;
use crate::state::AppState;

#[derive(Clone, Copy, PartialEq)]
pub enum Event {
    Queued,
    Completed,
    Failed,
}

impl Event {
    fn enabled(self, s: &Settings) -> bool {
        match self {
            Event::Queued => s.telegram_on_queued,
            Event::Completed => s.telegram_on_completed,
            Event::Failed => s.telegram_on_failed,
        }
    }
}

pub fn configured(s: &Settings) -> bool {
    !s.telegram_bot_token.trim().is_empty() && !s.telegram_chat_id.trim().is_empty()
}

pub fn html(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

pub fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

pub async fn send(bot_token: &str, chat_id: &str, topic_id: &str, silent: bool, title: &str, body_html: &str) -> Result<()> {
    let mut payload = json!({
        "chat_id": chat_id.trim(),
        "text": format!("<b>{}</b>\n{}", html(title), body_html),
        "parse_mode": "HTML",
        "disable_notification": silent,
        "disable_web_page_preview": true,
    });
    if let Ok(t) = topic_id.trim().parse::<i64>() {
        payload["message_thread_id"] = json!(t);
    }
    let client = reqwest::Client::builder().timeout(Duration::from_secs(20)).build()?;
    let r = client
        .post(format!("https://api.telegram.org/bot{}/sendMessage", bot_token.trim()))
        .json(&payload)
        .send()
        .await?;
    if !r.status().is_success() {
        let v: serde_json::Value = r.json().await.unwrap_or_default();
        bail!("Telegram: {}", v.get("description").and_then(|d| d.as_str()).unwrap_or("request failed"));
    }
    Ok(())
}

/// Fire-and-forget notification for an event, if enabled in the settings.
pub fn event(state: &AppState, ev: Event, title: &str, body_html: String) {
    let Ok(s) = state.db.settings() else { return };
    if !configured(&s) || !ev.enabled(&s) {
        return;
    }
    let st = state.clone();
    let title = format!("Converterr - {title}");
    tokio::spawn(async move {
        if let Err(e) = send(&s.telegram_bot_token, &s.telegram_chat_id, &s.telegram_topic_id, s.telegram_silent, &title, &body_html).await {
            st.log("warn", &format!("Telegram notification failed: {e:#}"));
        }
    });
}
