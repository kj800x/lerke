use crate::error::AppError;

pub struct Config {
    pub database_url: String,
    pub bind_address: String,
    pub discord_bot_token: String,
    pub discord_channel_id: u64,
    pub uptime_kuma_push_url: String,
    pub grafana_url: Option<String>,
    pub lerke_url: Option<String>,
    pub uptime_kuma_url: Option<String>,
    /// Base URL used for Grafana API calls (defaults to `grafana_url`). Lets the
    /// reconciler talk to an in-cluster address while links use the external one.
    pub grafana_api_url: Option<String>,
    /// Grafana service account token. When set (with a Grafana URL), Lerke polls the
    /// alertmanager API and resolves incidents whose alerts are no longer active.
    pub grafana_api_token: Option<String>,
    pub grafana_reconcile_interval_secs: u64,
}

impl Config {
    pub fn from_env() -> Result<Config, AppError> {
        let discord_channel_id: u64 = required("DISCORD_CHANNEL_ID")?
            .parse()
            .map_err(|e| AppError::Config(format!("DISCORD_CHANNEL_ID must be a u64: {}", e)))?;

        Ok(Config {
            database_url: optional("DATABASE_URL")
                .unwrap_or_else(|| "sqlite:data/lerke.db?mode=rwc".to_string()),
            bind_address: optional("BIND_ADDRESS").unwrap_or_else(|| "0.0.0.0:8080".to_string()),
            discord_bot_token: required("DISCORD_BOT_TOKEN")?,
            discord_channel_id,
            uptime_kuma_push_url: required("UPTIME_KUMA_PUSH_URL")?,
            grafana_url: optional("GRAFANA_URL").map(|u| u.trim_end_matches('/').to_string()),
            lerke_url: optional("LERKE_URL").map(|u| u.trim_end_matches('/').to_string()),
            uptime_kuma_url: optional("UPTIME_KUMA_URL")
                .map(|u| u.trim_end_matches('/').to_string()),
            grafana_api_url: optional("GRAFANA_API_URL")
                .map(|u| u.trim_end_matches('/').to_string()),
            grafana_api_token: optional("GRAFANA_API_TOKEN").filter(|t| !t.is_empty()),
            grafana_reconcile_interval_secs: match optional("GRAFANA_RECONCILE_INTERVAL_SECS") {
                Some(v) => v.parse().map_err(|e| {
                    AppError::Config(format!(
                        "GRAFANA_RECONCILE_INTERVAL_SECS must be a u64: {}",
                        e
                    ))
                })?,
                None => 60,
            },
        })
    }

    /// Settings for the Grafana reconcile loop, if it is configured.
    pub fn reconcile_settings(&self) -> Option<crate::reconcile::ReconcileSettings> {
        let token = self.grafana_api_token.clone()?;
        let api_url = self
            .grafana_api_url
            .clone()
            .or_else(|| self.grafana_url.clone())?;
        Some(crate::reconcile::ReconcileSettings {
            api_url,
            api_token: token,
            interval: std::time::Duration::from_secs(self.grafana_reconcile_interval_secs.max(5)),
        })
    }

    /// Rewrite a Grafana URL to use the configured GRAFANA_URL domain.
    /// Replaces the scheme+host+port portion, keeping the path and query.
    pub fn rewrite_grafana_url(&self, url: &str) -> String {
        match &self.grafana_url {
            Some(base) => {
                // Find the start of the path (after scheme://host[:port])
                if let Some(rest) = url
                    .find("://")
                    .and_then(|i| url[i + 3..].find('/').map(|j| &url[i + 3 + j..]))
                {
                    format!("{}{}", base, rest)
                } else {
                    url.to_string()
                }
            }
            None => url.to_string(),
        }
    }
}

fn required(name: &str) -> Result<String, AppError> {
    std::env::var(name).map_err(|_| AppError::Config(format!("{} must be set", name)))
}

fn optional(name: &str) -> Option<String> {
    std::env::var(name).ok()
}
