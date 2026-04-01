use actix_web::{post, web, HttpResponse, Responder};
use serde::Deserialize;

use crate::db::models::{AppState, WebhookLogEntry};
use crate::db::queries;
use crate::discord::notifier;
use crate::metrics;

#[derive(Debug, Deserialize)]
pub struct GrafanaWebhook {
    pub status: String,
    pub alerts: Vec<GrafanaAlert>,
    #[serde(default)]
    #[allow(dead_code)]
    pub receiver: String,
}

#[derive(Debug, Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrafanaAlert {
    pub status: String,
    pub labels: serde_json::Value,
    pub annotations: serde_json::Value,
    #[serde(default)]
    pub starts_at: String,
    #[serde(default)]
    pub ends_at: String,
    #[serde(default)]
    pub generator_url: Option<String>,
    pub fingerprint: String,
    #[serde(default)]
    pub silence_url: Option<String>,
    #[serde(default)]
    pub dashboard_url: Option<String>,
    #[serde(default)]
    pub panel_url: Option<String>,
}

impl GrafanaAlert {
    fn alert_name(&self) -> String {
        self.labels
            .get("alertname")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown Alert")
            .to_string()
    }

    fn severity(&self) -> Option<String> {
        self.labels
            .get("severity")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }
}

#[post("/api/webhooks/grafana")]
pub async fn grafana_webhook(
    body: web::Bytes,
    state: web::Data<AppState>,
) -> impl Responder {
    metrics::get().webhooks_received.add(1, &[]);

    let raw_body = String::from_utf8_lossy(&body).to_string();

    // Log the raw payload for debugging
    {
        let mut log = state.webhook_log.lock().await;
        log.push(WebhookLogEntry {
            received_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            raw_body: raw_body.clone(),
        });
    }

    let payload: GrafanaWebhook = match serde_json::from_str(&raw_body) {
        Ok(p) => p,
        Err(e) => {
            log::error!("Failed to parse Grafana webhook: {}", e);
            return HttpResponse::BadRequest().finish();
        }
    };

    log::info!(
        "Received Grafana webhook: status={}, alerts={}",
        payload.status,
        payload.alerts.len()
    );

    for alert in &payload.alerts {
        if let Err(e) = process_alert(alert, &state).await {
            log::error!("Failed to process alert {}: {}", alert.fingerprint, e);
        }
    }

    HttpResponse::Ok().finish()
}

async fn process_alert(alert: &GrafanaAlert, state: &AppState) -> Result<(), crate::error::AppError> {
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let alert_name = alert.alert_name();
    let raw_payload = serde_json::to_string(alert).ok();

    let existing = queries::get_incident_by_grafana_uid(&state.db, &alert.fingerprint).await?;

    match existing {
        None if alert.status == "firing" => {
            // New firing alert — create incident
            let incident_id = queries::create_incident(
                &state.db,
                &alert.fingerprint,
                &alert_name,
                "firing",
                alert.severity().as_deref(),
                alert.dashboard_url.as_deref(),
                alert.panel_url.as_deref(),
                alert.silence_url.as_deref(),
                &serde_json::to_string(&alert.labels).unwrap_or_default(),
                &serde_json::to_string(&alert.annotations).unwrap_or_default(),
                &now,
            )
            .await?;

            metrics::get().incidents_created.add(1, &[]);

            queries::create_incident_event(
                &state.db,
                incident_id,
                "firing",
                &format!("Alert {} started firing", alert_name),
                raw_payload.as_deref(),
            )
            .await?;

            // Fetch the created incident for Discord notification
            if let Some(incident) = queries::get_incident(&state.db, incident_id).await? {
                match notifier::send_firing_notification(
                    &state.discord_http,
                    state.config.discord_channel_id,
                    &incident,
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

            log::info!("Created new incident {} for alert {}", incident_id, alert_name);
        }

        Some(incident) if incident.status != alert.status => {
            // Status changed
            let resolved_at = if alert.status == "resolved" {
                Some(now.as_str())
            } else {
                None
            };

            queries::update_incident_status(
                &state.db,
                incident.id,
                &alert.status,
                resolved_at,
                &now,
            )
            .await?;

            let event_msg = if alert.status == "resolved" {
                metrics::get().incidents_resolved.add(1, &[]);
                format!("Alert {} resolved", alert_name)
            } else {
                format!("Alert {} status changed to {}", alert_name, alert.status)
            };

            queries::create_incident_event(
                &state.db,
                incident.id,
                &alert.status,
                &event_msg,
                raw_payload.as_deref(),
            )
            .await?;

            // Update Discord
            if let (Some(ref ch_id), Some(ref msg_id), Some(ref thread_id)) = (
                incident.discord_channel_id,
                incident.discord_message_id,
                incident.discord_thread_id,
            ) {
                // Fetch updated incident for embed
                if let Some(updated) = queries::get_incident(&state.db, incident.id).await? {
                    if let Err(e) =
                        notifier::update_incident_embed(&state.discord_http, ch_id, msg_id, &updated)
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

            log::info!(
                "Updated incident {} status to {}",
                incident.id,
                alert.status
            );
        }

        Some(incident) => {
            // Same status — just log the event
            queries::create_incident_event(
                &state.db,
                incident.id,
                &alert.status,
                &format!("Alert {} re-notified as {}", alert_name, alert.status),
                raw_payload.as_deref(),
            )
            .await?;
        }

        None => {
            // Non-firing alert with no existing incident — ignore
            log::debug!(
                "Ignoring {} alert for unknown fingerprint {}",
                alert.status,
                alert.fingerprint
            );
        }
    }

    Ok(())
}
