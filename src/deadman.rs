//! Dead man's switch: Lerke and Uptime Kuma watch each other.
//!
//! Every 5s Lerke pings an Uptime Kuma push monitor, so Kuma alerts if Lerke dies.
//! The reverse direction matters too: if Kuma is down or unreachable (box powered off,
//! network/IP change), nothing would be watching Lerke. So Lerke checks each heartbeat
//! gets a 2xx response, and after `FAILURES_TO_OPEN` consecutive failures (~1 minute)
//! opens its own incident. The first successful heartbeat afterwards resolves it.

use std::time::Duration;

use serde_json::json;

use crate::db::models::AppState;
use crate::db::queries;
use crate::incidents::{LERKE_UID_PREFIX, open_incident, resolve_incident};
use crate::metrics;

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(10);
/// 12 × 5s ≈ one minute of failed heartbeats before raising an incident.
const FAILURES_TO_OPEN: u32 = 12;
const ALERT_NAME: &str = "Uptime Kuma unreachable from Lerke";

fn deadman_uid() -> String {
    format!("{}deadman", LERKE_UID_PREFIX)
}

#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    None,
    Open,
    Resolve,
}

/// Tracks consecutive heartbeat failures and whether Lerke's incident is open.
#[derive(Debug, Default)]
pub struct Tracker {
    consecutive_failures: u32,
    open: bool,
}

impl Tracker {
    pub fn new(open: bool) -> Self {
        Tracker {
            consecutive_failures: 0,
            open,
        }
    }

    pub fn record(&mut self, ok: bool) -> Action {
        if ok {
            self.consecutive_failures = 0;
            if self.open {
                self.open = false;
                return Action::Resolve;
            }
            return Action::None;
        }
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        if !self.open && self.consecutive_failures >= FAILURES_TO_OPEN {
            self.open = true;
            return Action::Open;
        }
        Action::None
    }

    /// Undo a transition whose side effect failed, so the next heartbeat retries it.
    fn revert(&mut self, action: &Action) {
        match action {
            Action::Open => self.open = false,
            Action::Resolve => self.open = true,
            Action::None => {}
        }
    }
}

/// Send one heartbeat. Only a 2xx counts as success. Errors are returned without the
/// URL, since the push URL embeds Kuma's push token.
async fn heartbeat(client: &reqwest::Client, url: &str) -> Result<(), String> {
    match client.get(url).send().await {
        Ok(resp) if resp.status().is_success() => Ok(()),
        Ok(resp) => Err(format!("Uptime Kuma returned HTTP {}", resp.status())),
        Err(e) => Err(e.without_url().to_string()),
    }
}

async fn open(state: &AppState, error: &str) -> crate::error::AppResult<()> {
    let labels = json!({ "source": "lerke" });
    let mut annotations = json!({
        "summary": "Lerke's dead man's switch heartbeats to Uptime Kuma have failed for over a \
                    minute. Kuma may be down or unreachable, so nothing is watching Lerke right now.",
        "description": format!("Last error: {}", error),
    });
    if let Some(base) = &state.config.uptime_kuma_url {
        annotations["kuma_url"] = json!(base);
    }
    open_incident(
        state,
        &deadman_uid(),
        ALERT_NAME,
        &labels.to_string(),
        &annotations.to_string(),
        &format!("Heartbeats to Uptime Kuma failing: {}", error),
    )
    .await?;
    Ok(())
}

async fn resolve(state: &AppState) -> crate::error::AppResult<()> {
    if let Some(incident) = queries::get_incident_by_grafana_uid(&state.db, &deadman_uid()).await? {
        resolve_incident(
            state,
            &incident,
            "Heartbeats to Uptime Kuma are succeeding again",
        )
        .await?;
    }
    Ok(())
}

pub async fn heartbeat_loop(url: String, state: AppState) {
    let client = match reqwest::Client::builder()
        .timeout(HEARTBEAT_TIMEOUT)
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to build HTTP client for dead man's switch: {}", e);
            return;
        }
    };

    // Pick up an incident left open by a previous run so a success resolves it.
    let already_open = matches!(
        queries::get_incident_by_grafana_uid(&state.db, &deadman_uid()).await,
        Ok(Some(_))
    );
    let mut tracker = Tracker::new(already_open);

    let mut interval = tokio::time::interval(HEARTBEAT_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    log::info!("Starting dead man's switch heartbeat to Uptime Kuma");

    loop {
        interval.tick().await;
        let result = heartbeat(&client, &url).await;
        match &result {
            Ok(()) => metrics::get().deadman_heartbeats.add(1, &[]),
            Err(e) => {
                log::warn!("Dead man's switch heartbeat failed: {}", e);
                metrics::get().deadman_heartbeat_failures.add(1, &[]);
            }
        }

        let action = tracker.record(result.is_ok());
        let outcome = match (&action, &result) {
            (Action::Open, Err(e)) => open(&state, e).await,
            (Action::Resolve, _) => resolve(&state).await,
            _ => Ok(()),
        };
        if let Err(e) = outcome {
            log::error!("Dead man's switch failed to update its incident: {}", e);
            tracker.revert(&action);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_after_threshold_and_only_once() {
        let mut t = Tracker::new(false);
        for _ in 0..FAILURES_TO_OPEN - 1 {
            assert_eq!(t.record(false), Action::None);
        }
        assert_eq!(t.record(false), Action::Open);
        assert_eq!(t.record(false), Action::None);
    }

    #[test]
    fn success_resets_streak() {
        let mut t = Tracker::new(false);
        for _ in 0..FAILURES_TO_OPEN - 1 {
            t.record(false);
        }
        assert_eq!(t.record(true), Action::None);
        assert_eq!(t.record(false), Action::None);
    }

    #[test]
    fn resolves_on_first_success_after_open() {
        let mut t = Tracker::new(false);
        for _ in 0..FAILURES_TO_OPEN {
            t.record(false);
        }
        assert_eq!(t.record(true), Action::Resolve);
        assert_eq!(t.record(true), Action::None);
    }

    #[test]
    fn incident_left_open_by_previous_run_resolves() {
        let mut t = Tracker::new(true);
        assert_eq!(t.record(true), Action::Resolve);
    }

    #[test]
    fn failed_open_is_retried() {
        let mut t = Tracker::new(false);
        for _ in 0..FAILURES_TO_OPEN - 1 {
            t.record(false);
        }
        let action = t.record(false);
        assert_eq!(action, Action::Open);
        t.revert(&action);
        assert_eq!(t.record(false), Action::Open);
    }
}
