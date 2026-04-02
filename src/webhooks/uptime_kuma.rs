use actix_web::{post, web, HttpResponse, Responder};
use serde::Deserialize;

use crate::db::models::{AppState, WebhookLogEntry};
use crate::db::queries;
use crate::discord::notifier;
use crate::metrics;

#[derive(Debug, Deserialize)]
pub struct UptimeKumaWebhook {
    pub heartbeat: Option<UptimeKumaHeartbeat>,
    pub monitor: Option<UptimeKumaMonitor>,
    #[serde(default)]
    pub msg: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct UptimeKumaHeartbeat {
    #[serde(rename = "monitorID")]
    pub monitor_id: i64,
    pub status: i64, // 0=down, 1=up, 2=pending, 3=maintenance
    #[serde(default)]
    pub time: String,
    #[serde(default)]
    pub msg: String,
    pub ping: Option<i64>,
    #[serde(default)]
    pub important: bool,
    #[serde(default)]
    pub duration: i64,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct UptimeKumaMonitor {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub url: Option<String>,
    pub hostname: Option<String>,
    pub port: Option<i64>,
    #[serde(rename = "type")]
    pub monitor_type: Option<String>,
}

impl UptimeKumaHeartbeat {
    fn is_down(&self) -> bool {
        self.status == 0
    }

    fn is_up(&self) -> bool {
        self.status == 1
    }

    fn status_str(&self) -> &str {
        match self.status {
            0 => "firing",
            1 => "resolved",
            2 => "pending",
            3 => "maintenance",
            _ => "unknown",
        }
    }
}

#[post("/api/webhooks/uptime-kuma")]
pub async fn uptime_kuma_webhook(
    body: web::Bytes,
    state: web::Data<AppState>,
) -> impl Responder {
    metrics::get().webhooks_received.add(1, &[]);

    let raw_body = String::from_utf8_lossy(&body).to_string();

    // Log raw payload for debugging
    {
        let mut log = state.webhook_log.lock().await;
        log.push(WebhookLogEntry {
            received_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            raw_body: raw_body.clone(),
        });
    }

    let payload: UptimeKumaWebhook = match serde_json::from_str(&raw_body) {
        Ok(p) => p,
        Err(e) => {
            log::error!("Failed to parse Uptime Kuma webhook: {}", e);
            return HttpResponse::BadRequest().finish();
        }
    };

    let (Some(heartbeat), Some(monitor)) = (&payload.heartbeat, &payload.monitor) else {
        // Test notification — heartbeat/monitor can be null
        log::info!("Received Uptime Kuma test notification: {}", payload.msg);
        return HttpResponse::Ok().finish();
    };

    log::info!(
        "Received Uptime Kuma webhook: monitor={}, status={}",
        monitor.name,
        heartbeat.status_str()
    );

    if let Err(e) = process_heartbeat(heartbeat, monitor, &state).await {
        log::error!(
            "Failed to process Uptime Kuma heartbeat for {}: {}",
            monitor.name,
            e
        );
    }

    HttpResponse::Ok().finish()
}

async fn process_heartbeat(
    heartbeat: &UptimeKumaHeartbeat,
    monitor: &UptimeKumaMonitor,
    state: &AppState,
) -> Result<(), crate::error::AppError> {
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    // Use "uptime-kuma:<monitor_id>" as the unique identifier
    let uid = format!("uptime-kuma:{}", heartbeat.monitor_id);
    let alert_name = monitor.name.clone();
    let _status = heartbeat.status_str();

    // Build labels
    let mut labels = serde_json::Map::new();
    labels.insert("source".to_string(), serde_json::Value::String("uptime-kuma".to_string()));
    labels.insert("monitor_name".to_string(), serde_json::Value::String(monitor.name.clone()));
    if let Some(ref mt) = monitor.monitor_type {
        labels.insert("monitor_type".to_string(), serde_json::Value::String(mt.clone()));
    }

    // Build annotations
    let mut annotations = serde_json::Map::new();
    if let Some(ref desc) = monitor.description {
        if !desc.is_empty() {
            annotations.insert("description".to_string(), serde_json::Value::String(desc.clone()));
        }
    }
    if !heartbeat.msg.is_empty() {
        annotations.insert("summary".to_string(), serde_json::Value::String(heartbeat.msg.clone()));
    }
    if let Some(ref url) = monitor.url {
        if !url.is_empty() && url != "https://" {
            annotations.insert("site_url".to_string(), serde_json::Value::String(url.clone()));
        }
    }

    // Uptime Kuma dashboard link — stored as annotation so it renders as "Kuma" link
    if let Some(ref base) = state.config.uptime_kuma_url {
        annotations.insert(
            "kuma_url".to_string(),
            serde_json::Value::String(format!("{}/dashboard/{}", base, monitor.id)),
        );
    }

    let labels_json = serde_json::to_string(&serde_json::Value::Object(labels)).unwrap_or_default();
    let annotations_json = serde_json::to_string(&serde_json::Value::Object(annotations)).unwrap_or_default();

    let existing = queries::get_incident_by_grafana_uid(&state.db, &uid).await?;

    match existing {
        None if heartbeat.is_down() => {
            let incident_id = queries::create_incident(
                &state.db,
                &uid,
                &alert_name,
                "firing",
                None,
                None, // dashboard_url
                None,                    // panel_url
                None,                    // silence_url
                None,                    // generator_url
                &labels_json,
                &annotations_json,
                &now,
            )
            .await?;

            metrics::get().incidents_created.add(1, &[]);

            queries::create_incident_event(
                &state.db,
                incident_id,
                "firing",
                &format!("{} is down: {}", alert_name, heartbeat.msg),
                None,
            )
            .await?;

            if let Some(incident) = queries::get_incident(&state.db, incident_id).await? {
                match notifier::send_firing_notification(
                    &state.discord_http,
                    state.config.discord_channel_id,
                    &incident,
                    state.config.lerke_url.as_deref(),
                )
                .await
                {
                    Ok((message_id, channel_id, thread_id)) => {
                        queries::update_incident_discord(
                            &state.db,
                            incident_id,
                            &message_id,
                            &channel_id,
                            &thread_id,
                        )
                        .await?;
                        metrics::get().discord_notifications_sent.add(1, &[]);
                    }
                    Err(e) => {
                        log::error!("Failed to send Discord notification: {}", e);
                        metrics::get().discord_notification_errors.add(1, &[]);
                    }
                }
            }

            log::info!("Created Uptime Kuma incident {} for {}", incident_id, alert_name);
        }

        Some(incident) if incident.status == "firing" && heartbeat.is_up() => {
            queries::update_incident_status(&state.db, incident.id, "resolved", Some(&now), &now)
                .await?;

            metrics::get().incidents_resolved.add(1, &[]);

            let event_msg = "Alert resolved".to_string();

            queries::create_incident_event(
                &state.db,
                incident.id,
                "resolved",
                &event_msg,
                None,
            )
            .await?;

            if let (Some(ch_id), Some(msg_id), Some(thread_id)) = (
                &incident.discord_channel_id,
                &incident.discord_message_id,
                &incident.discord_thread_id,
            ) {
                if let Some(updated) = queries::get_incident(&state.db, incident.id).await? {
                    if let Err(e) = notifier::update_incident_embed(
                        &state.discord_http,
                        ch_id,
                        msg_id,
                        &updated,
                        state.config.lerke_url.as_deref(),
                    )
                    .await
                    {
                        log::error!("Failed to update Discord embed: {}", e);
                        metrics::get().discord_notification_errors.add(1, &[]);
                    }
                }

                if let Err(e) =
                    notifier::post_thread_update(&state.discord_http, thread_id, &event_msg).await
                {
                    log::error!("Failed to post Discord thread update: {}", e);
                    metrics::get().discord_notification_errors.add(1, &[]);
                }
            }

            log::info!("Resolved Uptime Kuma incident {} for {}", incident.id, alert_name);
        }

        Some(incident) if heartbeat.is_down() && heartbeat.important => {
            // Re-notified as down while already firing — log event
            queries::create_incident_event(
                &state.db,
                incident.id,
                "firing",
                &format!("Still down: {}", heartbeat.msg),
                None,
            )
            .await?;
        }

        None if heartbeat.is_up() => {
            // Up with no open incident — ignore
            log::debug!("Ignoring UP heartbeat for {} with no open incident", alert_name);
        }

        _ => {}
    }

    Ok(())
}
