use std::collections::VecDeque;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tokio::sync::Mutex;

use crate::config::Config;

pub struct WebhookLogEntry {
    pub received_at: String,
    pub raw_body: String,
}

pub struct WebhookLog {
    entries: VecDeque<WebhookLogEntry>,
    max_entries: usize,
}

impl WebhookLog {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            max_entries,
        }
    }

    pub fn push(&mut self, entry: WebhookLogEntry) {
        if self.entries.len() >= self.max_entries {
            self.entries.pop_back();
        }
        self.entries.push_front(entry);
    }

    pub fn entries(&self) -> &VecDeque<WebhookLogEntry> {
        &self.entries
    }
}

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub discord_http: Arc<serenity::http::Http>,
    pub config: Arc<Config>,
    pub webhook_log: Arc<Mutex<WebhookLog>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Incident {
    pub id: i64,
    pub grafana_alert_uid: String,
    pub alert_name: String,
    pub status: String,
    pub severity: Option<String>,
    pub grafana_dashboard_url: Option<String>,
    pub grafana_panel_url: Option<String>,
    pub grafana_silence_url: Option<String>,
    pub labels_json: String,
    pub annotations_json: String,
    pub discord_message_id: Option<String>,
    pub discord_channel_id: Option<String>,
    pub discord_thread_id: Option<String>,
    pub first_firing_at: String,
    pub last_status_change_at: String,
    pub resolved_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct IncidentEvent {
    pub id: i64,
    pub incident_id: i64,
    pub event_type: String,
    pub message: String,
    pub raw_payload_json: Option<String>,
    pub created_at: String,
}
