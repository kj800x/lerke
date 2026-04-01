CREATE TABLE IF NOT EXISTS incidents (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    grafana_alert_uid TEXT NOT NULL,
    alert_name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'firing',
    severity TEXT,
    grafana_dashboard_url TEXT,
    grafana_panel_url TEXT,
    grafana_silence_url TEXT,
    labels_json TEXT NOT NULL DEFAULT '{}',
    annotations_json TEXT NOT NULL DEFAULT '{}',
    discord_message_id TEXT,
    discord_channel_id TEXT,
    discord_thread_id TEXT,
    first_firing_at TEXT NOT NULL,
    last_status_change_at TEXT NOT NULL,
    resolved_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_incidents_grafana_uid ON incidents(grafana_alert_uid);
CREATE INDEX idx_incidents_status ON incidents(status);
CREATE INDEX idx_incidents_created_at ON incidents(created_at);

CREATE TABLE IF NOT EXISTS incident_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    incident_id INTEGER NOT NULL REFERENCES incidents(id),
    event_type TEXT NOT NULL,
    message TEXT NOT NULL,
    raw_payload_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_incident_events_incident_id ON incident_events(incident_id);
