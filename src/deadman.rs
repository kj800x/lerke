use crate::metrics;

pub async fn heartbeat_loop(url: String) {
    let client = reqwest::Client::new();
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));

    log::info!("Starting dead man's switch heartbeat to {}", url);

    loop {
        interval.tick().await;
        match client.get(&url).send().await {
            Ok(_) => {
                metrics::get().deadman_heartbeats.add(1, &[]);
            }
            Err(e) => {
                log::warn!("Dead man's switch heartbeat failed: {}", e);
            }
        }
    }
}
