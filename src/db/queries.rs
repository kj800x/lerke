use sqlx::SqlitePool;

use super::models::{Incident, IncidentEvent};
use crate::error::AppResult;

pub async fn list_incidents(
    pool: &SqlitePool,
    status_filter: Option<&str>,
) -> AppResult<Vec<Incident>> {
    let incidents = match status_filter {
        Some(status) => {
            sqlx::query_as::<_, Incident>(
                "SELECT * FROM incidents WHERE status = ? ORDER BY last_status_change_at DESC",
            )
            .bind(status)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query_as::<_, Incident>(
                "SELECT * FROM incidents ORDER BY last_status_change_at DESC",
            )
            .fetch_all(pool)
            .await?
        }
    };
    Ok(incidents)
}

pub async fn get_incident(pool: &SqlitePool, id: i64) -> AppResult<Option<Incident>> {
    let incident =
        sqlx::query_as::<_, Incident>("SELECT * FROM incidents WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    Ok(incident)
}

pub async fn get_incident_by_grafana_uid(
    pool: &SqlitePool,
    uid: &str,
) -> AppResult<Option<Incident>> {
    let incident = sqlx::query_as::<_, Incident>(
        "SELECT * FROM incidents WHERE grafana_alert_uid = ? AND status != 'resolved' ORDER BY created_at DESC LIMIT 1",
    )
    .bind(uid)
    .fetch_optional(pool)
    .await?;
    Ok(incident)
}

pub async fn create_incident(
    pool: &SqlitePool,
    grafana_alert_uid: &str,
    alert_name: &str,
    status: &str,
    severity: Option<&str>,
    grafana_dashboard_url: Option<&str>,
    grafana_panel_url: Option<&str>,
    grafana_silence_url: Option<&str>,
    grafana_generator_url: Option<&str>,
    labels_json: &str,
    annotations_json: &str,
    now: &str,
) -> AppResult<i64> {
    let result = sqlx::query(
        "INSERT INTO incidents (grafana_alert_uid, alert_name, status, severity, \
         grafana_dashboard_url, grafana_panel_url, grafana_silence_url, grafana_generator_url, \
         labels_json, annotations_json, first_firing_at, last_status_change_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(grafana_alert_uid)
    .bind(alert_name)
    .bind(status)
    .bind(severity)
    .bind(grafana_dashboard_url)
    .bind(grafana_panel_url)
    .bind(grafana_silence_url)
    .bind(grafana_generator_url)
    .bind(labels_json)
    .bind(annotations_json)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(result.last_insert_rowid())
}

pub async fn update_incident_status(
    pool: &SqlitePool,
    id: i64,
    status: &str,
    resolved_at: Option<&str>,
    now: &str,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE incidents SET status = ?, resolved_at = ?, last_status_change_at = ? WHERE id = ?",
    )
    .bind(status)
    .bind(resolved_at)
    .bind(now)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn update_incident_discord(
    pool: &SqlitePool,
    id: i64,
    message_id: &str,
    channel_id: &str,
    thread_id: &str,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE incidents SET discord_message_id = ?, discord_channel_id = ?, discord_thread_id = ? WHERE id = ?",
    )
    .bind(message_id)
    .bind(channel_id)
    .bind(thread_id)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_incident_events(
    pool: &SqlitePool,
    incident_id: i64,
) -> AppResult<Vec<IncidentEvent>> {
    let events = sqlx::query_as::<_, IncidentEvent>(
        "SELECT * FROM incident_events WHERE incident_id = ? ORDER BY created_at ASC",
    )
    .bind(incident_id)
    .fetch_all(pool)
    .await?;
    Ok(events)
}

pub async fn create_incident_event(
    pool: &SqlitePool,
    incident_id: i64,
    event_type: &str,
    message: &str,
    raw_payload_json: Option<&str>,
) -> AppResult<i64> {
    let result = sqlx::query(
        "INSERT INTO incident_events (incident_id, event_type, message, raw_payload_json) \
         VALUES (?, ?, ?, ?)",
    )
    .bind(incident_id)
    .bind(event_type)
    .bind(message)
    .bind(raw_payload_json)
    .execute(pool)
    .await?;
    Ok(result.last_insert_rowid())
}
