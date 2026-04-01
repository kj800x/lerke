use std::sync::OnceLock;

use opentelemetry::{global, metrics::Counter};

pub struct Metrics {
    pub webhooks_received: Counter<u64>,
    pub incidents_created: Counter<u64>,
    pub incidents_resolved: Counter<u64>,
    pub discord_notifications_sent: Counter<u64>,
    pub discord_notification_errors: Counter<u64>,
    pub deadman_heartbeats: Counter<u64>,
}

static METRICS: OnceLock<Metrics> = OnceLock::new();

pub fn init(_registry: &prometheus::Registry) -> Result<(), anyhow::Error> {
    let meter = global::meter("lerke");

    let metrics = Metrics {
        webhooks_received: meter.u64_counter("lerke_webhooks_received").init(),
        incidents_created: meter.u64_counter("lerke_incidents_created").init(),
        incidents_resolved: meter.u64_counter("lerke_incidents_resolved").init(),
        discord_notifications_sent: meter
            .u64_counter("lerke_discord_notifications_sent")
            .init(),
        discord_notification_errors: meter
            .u64_counter("lerke_discord_notification_errors")
            .init(),
        deadman_heartbeats: meter.u64_counter("lerke_deadman_heartbeats").init(),
    };

    METRICS
        .set(metrics)
        .map_err(|_| anyhow::anyhow!("Metrics already initialized"))?;

    Ok(())
}

#[allow(clippy::expect_used)]
pub fn get() -> &'static Metrics {
    METRICS
        .get()
        .expect("Metrics not initialized - call metrics::init() first")
}
