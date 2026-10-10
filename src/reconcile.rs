//! Periodic reconciliation of firing incidents against Grafana's alertmanager.
//!
//! Lerke normally resolves incidents when Grafana sends a `resolved` webhook. If that
//! webhook is lost (Grafana restart, network blip, Lerke down), the incident stays
//! firing forever. This loop polls Grafana's alertmanager API for the set of alerts it
//! currently considers active and resolves any firing Grafana incident whose
//! fingerprint is no longer in that set.
//!
//! To avoid resolving incidents on a transient glitch (e.g. Grafana returning an
//! empty list while it is still starting up), an incident is only resolved after its
//! fingerprint has been missing in two consecutive successful polls.

use std::collections::HashSet;
use std::time::Duration;

use serde::Deserialize;

use crate::db::models::{AppState, Incident};
use crate::db::queries;
use crate::incidents::{UPTIME_KUMA_UID_PREFIX, resolve_incident};
use crate::metrics;

const ALERTS_PATH: &str = "/api/alertmanager/grafana/api/v2/alerts";

#[derive(Debug, Deserialize)]
struct AlertmanagerAlert {
    fingerprint: String,
}

pub struct ReconcileSettings {
    pub api_url: String,
    pub api_token: String,
    pub interval: Duration,
}

/// Returns the firing Grafana incidents (not Uptime Kuma) whose fingerprint is absent
/// from `active` and was also absent in the previous poll (`missing_last_poll`).
pub fn select_stale<'a>(
    firing: &'a [Incident],
    active: &HashSet<String>,
    missing_last_poll: &HashSet<String>,
) -> Vec<&'a Incident> {
    firing
        .iter()
        .filter(|i| !i.grafana_alert_uid.starts_with(UPTIME_KUMA_UID_PREFIX))
        .filter(|i| !active.contains(&i.grafana_alert_uid))
        .filter(|i| missing_last_poll.contains(&i.grafana_alert_uid))
        .collect()
}

/// Fingerprints of firing Grafana incidents that are absent from `active` right now.
pub fn currently_missing(firing: &[Incident], active: &HashSet<String>) -> HashSet<String> {
    firing
        .iter()
        .filter(|i| !i.grafana_alert_uid.starts_with(UPTIME_KUMA_UID_PREFIX))
        .filter(|i| !active.contains(&i.grafana_alert_uid))
        .map(|i| i.grafana_alert_uid.clone())
        .collect()
}

async fn fetch_active_fingerprints(
    client: &reqwest::Client,
    settings: &ReconcileSettings,
) -> Result<HashSet<String>, crate::error::AppError> {
    let url = format!("{}{}", settings.api_url, ALERTS_PATH);
    let resp = client
        .get(&url)
        .bearer_auth(&settings.api_token)
        .send()
        .await?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(crate::error::AppError::Internal(format!(
            "Grafana alertmanager API returned {}: {}",
            status,
            body.chars().take(200).collect::<String>()
        )));
    }

    let alerts: Vec<AlertmanagerAlert> = resp.json().await?;
    Ok(alerts.into_iter().map(|a| a.fingerprint).collect())
}

/// One reconcile pass. Returns the set of fingerprints missing from Grafana this pass,
/// which the caller feeds back in as `missing_last_poll` next time.
pub async fn reconcile_once(
    client: &reqwest::Client,
    settings: &ReconcileSettings,
    state: &AppState,
    missing_last_poll: &HashSet<String>,
) -> Result<HashSet<String>, crate::error::AppError> {
    let active = fetch_active_fingerprints(client, settings).await?;
    let firing = queries::list_incidents(&state.db, Some("firing")).await?;

    let stale = select_stale(&firing, &active, missing_last_poll);
    let missing_now = currently_missing(&firing, &active);

    log::debug!(
        "Reconcile: {} active in Grafana, {} firing in Lerke, {} missing, {} stale",
        active.len(),
        firing.len(),
        missing_now.len(),
        stale.len()
    );

    for incident in stale {
        let msg =
            "Alert resolved (no longer active in Grafana; resolve webhook was never received)";
        match resolve_incident(state, incident, msg).await {
            Ok(()) => {
                metrics::get().incidents_reconciled.add(1, &[]);
            }
            Err(e) => {
                log::error!(
                    "Reconcile failed to resolve incident {}: {}",
                    incident.id,
                    e
                );
            }
        }
    }

    Ok(missing_now)
}

pub async fn reconcile_loop(settings: ReconcileSettings, state: AppState) {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to build HTTP client for Grafana reconcile: {}", e);
            return;
        }
    };

    let mut interval = tokio::time::interval(settings.interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    log::info!(
        "Starting Grafana reconcile loop against {} every {:?}",
        settings.api_url,
        settings.interval
    );

    let mut missing_last_poll = HashSet::new();

    loop {
        interval.tick().await;
        match reconcile_once(&client, &settings, &state, &missing_last_poll).await {
            Ok(missing) => missing_last_poll = missing,
            Err(e) => {
                // Keep the previous "missing" set so one failed poll doesn't reset the
                // two-strikes counter, but never resolve anything on a failed poll.
                log::warn!("Grafana reconcile failed: {}", e);
                metrics::get().reconcile_errors.add(1, &[]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn incident(uid: &str) -> Incident {
        Incident {
            id: 0,
            grafana_alert_uid: uid.to_string(),
            alert_name: uid.to_string(),
            status: "firing".to_string(),
            severity: None,
            grafana_dashboard_url: None,
            grafana_panel_url: None,
            grafana_silence_url: None,
            labels_json: "{}".to_string(),
            annotations_json: "{}".to_string(),
            discord_message_id: None,
            discord_channel_id: None,
            discord_thread_id: None,
            first_firing_at: String::new(),
            last_status_change_at: String::new(),
            resolved_at: None,
            created_at: String::new(),
            grafana_generator_url: None,
        }
    }

    fn set(items: &[&str]) -> HashSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn resolves_only_after_two_consecutive_misses() {
        let firing = vec![incident("aaa"), incident("bbb")];
        let active = set(&["bbb"]);

        // First miss: nothing resolved, but "aaa" is recorded as missing.
        let stale = select_stale(&firing, &active, &HashSet::new());
        assert!(stale.is_empty());
        let missing = currently_missing(&firing, &active);
        assert_eq!(missing, set(&["aaa"]));

        // Second miss: "aaa" is stale.
        let stale = select_stale(&firing, &active, &missing);
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].grafana_alert_uid, "aaa");
    }

    #[test]
    fn reappearing_alert_is_not_resolved() {
        let firing = vec![incident("aaa")];
        let missing_before = set(&["aaa"]);
        let active = set(&["aaa"]);
        assert!(select_stale(&firing, &active, &missing_before).is_empty());
        assert!(currently_missing(&firing, &active).is_empty());
    }

    #[test]
    fn uptime_kuma_incidents_are_ignored() {
        let firing = vec![incident("uptime-kuma:42")];
        let missing_before = set(&["uptime-kuma:42"]);
        assert!(select_stale(&firing, &HashSet::new(), &missing_before).is_empty());
        assert!(currently_missing(&firing, &HashSet::new()).is_empty());
    }
}
