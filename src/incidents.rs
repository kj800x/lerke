//! Shared incident lifecycle operations used by webhooks, the reconciler, and the web UI.

use crate::db::models::{AppState, Incident};
use crate::db::queries;
use crate::discord::notifier;
use crate::error::AppResult;
use crate::metrics;

/// Prefix used for Uptime Kuma incident UIDs (see `webhooks::uptime_kuma`).
pub const UPTIME_KUMA_UID_PREFIX: &str = "uptime-kuma:";

/// Mark a firing incident as resolved, record an event, and update Discord.
///
/// `event_msg` is stored on the incident timeline and posted to the Discord thread,
/// so it should say *why* the incident was resolved (webhook, reconcile, manual).
pub async fn resolve_incident(
    state: &AppState,
    incident: &Incident,
    event_msg: &str,
) -> AppResult<()> {
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    queries::update_incident_status(&state.db, incident.id, "resolved", Some(&now), &now).await?;
    metrics::get().incidents_resolved.add(1, &[]);

    queries::create_incident_event(&state.db, incident.id, "resolved", event_msg, None).await?;

    if let (Some(ch_id), Some(msg_id), Some(thread_id)) = (
        &incident.discord_channel_id,
        &incident.discord_message_id,
        &incident.discord_thread_id,
    ) {
        if let Some(updated) = queries::get_incident(&state.db, incident.id).await?
            && let Err(e) = notifier::update_incident_embed(
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

        if let Err(e) =
            notifier::post_thread_update(&state.discord_http, thread_id, event_msg).await
        {
            log::error!("Failed to post Discord thread update: {}", e);
            metrics::get().discord_notification_errors.add(1, &[]);
        }
    }

    log::info!(
        "Resolved incident {} ({}): {}",
        incident.id,
        incident.alert_name,
        event_msg
    );

    Ok(())
}
