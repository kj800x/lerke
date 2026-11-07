//! Daily "still open" digest.
//!
//! Lerke posts a Discord message when an incident starts firing and edits it when it
//! resolves, but an incident that stays open for days never gets mentioned again. Once
//! a day at `DIGEST_TIME` (Eastern, default 18:00) this loop posts a single channel
//! message listing every incident that is still firing, so long-running ones don't get
//! forgotten. Nothing is posted on days with no open incidents.
//!
//! Each handled day is recorded in the `digests` table, so a restart never sends the
//! same day's digest twice, and if Lerke was down at digest time the digest goes out as
//! soon as it comes back (the same evening).

use std::time::Duration;

use chrono::{DateTime, NaiveDateTime, NaiveTime, TimeZone, Utc};
use chrono_tz::America::New_York;
use chrono_tz::Tz;
use serenity::all::{ChannelId, CreateMessage};

use crate::db::models::{AppState, Incident};
use crate::db::queries;
use crate::error::{AppError, AppResult};
use crate::metrics;

/// Discord's message content limit.
const DISCORD_MAX_CONTENT: usize = 2000;

/// The digest is due once local time has reached `digest_time` and today's digest
/// hasn't been handled yet.
pub fn digest_due(now_local: &DateTime<Tz>, digest_time: NaiveTime, already_sent: bool) -> bool {
    !already_sent && now_local.time() >= digest_time
}

fn parse_utc(ts: &str) -> Option<DateTime<Utc>> {
    NaiveDateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%SZ")
        .or_else(|_| NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S"))
        .ok()
        .map(|n| Utc.from_utc_datetime(&n))
}

/// Compact age like "3d 4h", "5h 12m", "8m".
pub fn format_age(age: chrono::Duration) -> String {
    let mins = age.num_minutes().max(0);
    let (days, hours, minutes) = (mins / 1440, (mins % 1440) / 60, mins % 60);
    if days > 0 {
        format!("{}d {}h", days, hours)
    } else if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else {
        format!("{}m", minutes)
    }
}

/// Build the digest message for the given firing incidents, oldest first. Returns
/// `None` when there is nothing open.
pub fn build_digest(
    incidents: &[Incident],
    now: DateTime<Utc>,
    lerke_url: Option<&str>,
) -> Option<String> {
    if incidents.is_empty() {
        return None;
    }

    let mut sorted: Vec<&Incident> = incidents.iter().collect();
    sorted.sort_by(|a, b| a.first_firing_at.cmp(&b.first_firing_at));

    let noun = if sorted.len() == 1 {
        "incident"
    } else {
        "incidents"
    };
    let mut content = format!("**Still open: {} {}**\n", sorted.len(), noun);

    for (idx, incident) in sorted.iter().enumerate() {
        let mut line = format!("- **{}**", incident.alert_name);
        if let Some(since) = parse_utc(&incident.first_firing_at) {
            line.push_str(&format!(" · firing {}", format_age(now - since)));
        }
        if let Some(thread) = &incident.discord_thread_id {
            line.push_str(&format!(" · <#{}>", thread));
        }
        if let Some(base) = lerke_url {
            // <...> suppresses Discord's link preview.
            line.push_str(&format!(" · [Lerke](<{}/incidents/{}>)", base, incident.id));
        }
        line.push('\n');

        let remaining = sorted.len() - idx;
        let more = format!("…and {} more\n", remaining);
        // Leave room for the "and N more" trailer in case a later line won't fit.
        if content.len() + line.len() + more.len() > DISCORD_MAX_CONTENT {
            content.push_str(&more);
            break;
        }
        content.push_str(&line);
    }

    Some(content)
}

async fn send_digest(state: &AppState, content: &str) -> AppResult<()> {
    ChannelId::new(state.config.discord_channel_id)
        .send_message(&state.discord_http, CreateMessage::new().content(content))
        .await
        .map_err(|e| AppError::Discord(format!("Failed to send digest: {}", e)))?;
    Ok(())
}

/// One check: send today's digest if it's due. Returns whether a digest was handled.
pub async fn digest_once(state: &AppState, digest_time: NaiveTime) -> AppResult<bool> {
    let now = Utc::now();
    let now_local = now.with_timezone(&New_York);
    let today = now_local.format("%Y-%m-%d").to_string();

    let already_sent = queries::digest_sent(&state.db, &today).await?;
    if !digest_due(&now_local, digest_time, already_sent) {
        return Ok(false);
    }

    let firing = queries::list_incidents(&state.db, Some("firing")).await?;
    if let Some(content) = build_digest(&firing, now, state.config.lerke_url.as_deref()) {
        send_digest(state, &content).await?;
        metrics::get().discord_notifications_sent.add(1, &[]);
        metrics::get().digests_sent.add(1, &[]);
        log::info!("Sent daily digest with {} open incident(s)", firing.len());
    } else {
        log::info!("Daily digest: no open incidents, nothing to send");
    }

    let sent_at = now.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    queries::record_digest(&state.db, &today, &sent_at, firing.len() as i64).await?;
    Ok(true)
}

pub async fn digest_loop(digest_time: NaiveTime, state: AppState) {
    log::info!(
        "Starting daily digest loop (posts at {} America/New_York)",
        digest_time.format("%H:%M")
    );

    let mut interval = tokio::time::interval(Duration::from_secs(60));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        interval.tick().await;
        // On failure (e.g. Discord error) nothing is recorded, so the next tick retries.
        if let Err(e) = digest_once(&state, digest_time).await {
            log::warn!("Daily digest failed: {}", e);
            metrics::get().discord_notification_errors.add(1, &[]);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn incident(id: i64, name: &str, first_firing_at: &str, thread: Option<&str>) -> Incident {
        Incident {
            id,
            grafana_alert_uid: format!("uid-{}", id),
            alert_name: name.to_string(),
            status: "firing".to_string(),
            severity: None,
            grafana_dashboard_url: None,
            grafana_panel_url: None,
            grafana_silence_url: None,
            labels_json: "{}".to_string(),
            annotations_json: "{}".to_string(),
            discord_message_id: None,
            discord_channel_id: None,
            discord_thread_id: thread.map(str::to_string),
            first_firing_at: first_firing_at.to_string(),
            last_status_change_at: first_firing_at.to_string(),
            resolved_at: None,
            created_at: String::new(),
            grafana_generator_url: None,
        }
    }

    fn utc(ts: &str) -> DateTime<Utc> {
        parse_utc(ts).unwrap()
    }

    fn six_pm() -> NaiveTime {
        NaiveTime::from_hms_opt(18, 0, 0).unwrap()
    }

    #[test]
    fn due_only_after_digest_time_and_once_per_day() {
        // 22:59 UTC on Nov 6 = 17:59 EST.
        let before = utc("2026-11-06T22:59:00Z").with_timezone(&New_York);
        let after = utc("2026-11-06T23:00:00Z").with_timezone(&New_York);
        assert!(!digest_due(&before, six_pm(), false));
        assert!(digest_due(&after, six_pm(), false));
        assert!(!digest_due(&after, six_pm(), true));
    }

    #[test]
    fn age_formatting() {
        assert_eq!(format_age(chrono::Duration::minutes(8)), "8m");
        assert_eq!(format_age(chrono::Duration::minutes(5 * 60 + 12)), "5h 12m");
        assert_eq!(
            format_age(chrono::Duration::minutes(3 * 1440 + 4 * 60)),
            "3d 4h"
        );
    }

    #[test]
    fn empty_digest_is_none() {
        assert!(build_digest(&[], Utc::now(), None).is_none());
    }

    #[test]
    fn digest_lists_oldest_first_with_links() {
        let now = utc("2026-11-07T23:00:00Z");
        let incidents = vec![
            incident(2, "Newer", "2026-11-07T20:00:00Z", None),
            incident(1, "Older", "2026-11-04T23:00:00Z", Some("123")),
        ];
        let msg = build_digest(&incidents, now, Some("https://lerke.example")).unwrap();
        assert_eq!(
            msg,
            "**Still open: 2 incidents**\n\
             - **Older** · firing 3d 0h · <#123> · [Lerke](<https://lerke.example/incidents/1>)\n\
             - **Newer** · firing 3h 0m · [Lerke](<https://lerke.example/incidents/2>)\n"
        );
    }

    #[test]
    fn digest_truncates_to_discord_limit() {
        let now = utc("2026-11-07T23:00:00Z");
        let long_name = "x".repeat(150);
        let incidents: Vec<Incident> = (0..40)
            .map(|i| incident(i, &long_name, "2026-11-07T20:00:00Z", Some("123")))
            .collect();
        let msg = build_digest(&incidents, now, None).unwrap();
        assert!(msg.len() <= DISCORD_MAX_CONTENT);
        assert!(msg.contains("more\n"));
    }
}
