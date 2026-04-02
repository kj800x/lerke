use serenity::all::{
    ChannelId, CreateEmbed, CreateMessage, CreateThread, EditMessage, MessageId,
};
use serenity::http::Http;

use crate::db::models::Incident;
use crate::error::AppError;

const COLOR_RED: u32 = 0xFF0000;
const COLOR_GREEN: u32 = 0x00CC00;

pub const FILTERED_LABELS: &[&str] = &["alertname", "grafana_folder", "priority"];

fn get_annotation(incident: &Incident, key: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(&incident.annotations_json)
        .ok()
        .and_then(|v| v.get(key)?.as_str().map(|s| s.to_string()))
}

/// Collect annotations that end in _url as (label, url) pairs
fn collect_url_annotations(incident: &Incident) -> Vec<(String, String)> {
    let mut urls = Vec::new();
    if let Ok(annotations) = serde_json::from_str::<serde_json::Value>(&incident.annotations_json) {
        if let Some(obj) = annotations.as_object() {
            for (key, value) in obj {
                if let Some(prefix) = key.strip_suffix("_url") {
                    if let Some(url) = value.as_str() {
                        urls.push((url_key_to_label(prefix), url.to_string()));
                    }
                }
            }
        }
    }
    urls
}

/// Convert a snake_case key to Sentence case label (e.g. "jobfather" -> "Jobfather")
pub fn url_key_to_label(key: &str) -> String {
    let label = key.replace('_', " ");
    let mut chars = label.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

fn build_embed(incident: &Incident, lerke_url: Option<&str>) -> CreateEmbed {
    let color = if incident.status == "resolved" {
        COLOR_GREEN
    } else {
        COLOR_RED
    };

    let mut embed = CreateEmbed::new()
        .title(&incident.alert_name)
        .color(color);

    // Build description: summary, then links
    let mut desc_parts = Vec::new();

    if let Some(summary) = get_annotation(incident, "summary") {
        desc_parts.push(summary);
    }
    if let Some(description) = get_annotation(incident, "description") {
        desc_parts.push(description);
    }

    let mut links = Vec::new();
    if let Some(base) = lerke_url {
        links.push(format!("[Lerke]({}/incidents/{})", base, incident.id));
    }
    for (label, url) in collect_url_annotations(incident) {
        links.push(format!("[{}]({})", label, url));
    }
    if let Some(ref url) = incident.grafana_dashboard_url {
        links.push(format!("[Dashboard]({})", url));
    }
    if let Some(ref url) = incident.grafana_panel_url {
        links.push(format!("[Panel]({})", url));
    }
    if !links.is_empty() {
        desc_parts.push(links.join(" · "));
    }

    if !desc_parts.is_empty() {
        embed = embed.description(desc_parts.join("\n\n"));
    }

    // Add alert labels as fields (vertical — inline=false), filtering out noise
    if let Ok(labels) = serde_json::from_str::<serde_json::Value>(&incident.labels_json) {
        if let Some(obj) = labels.as_object() {
            for (key, value) in obj {
                if FILTERED_LABELS.contains(&key.as_str()) {
                    continue;
                }
                let val_str = value.as_str().unwrap_or(&value.to_string()).to_string();
                embed = embed.field(key, &val_str, false);
            }
        }
    }

    embed
}

pub async fn send_firing_notification(
    http: &Http,
    channel_id: u64,
    incident: &Incident,
    lerke_url: Option<&str>,
) -> Result<(String, String, String), AppError> {
    let channel = ChannelId::new(channel_id);
    let embed = build_embed(incident, lerke_url);

    let message = channel
        .send_message(http, CreateMessage::new().embed(embed))
        .await
        .map_err(|e| AppError::Discord(format!("Failed to send message: {}", e)))?;

    let thread_name = if incident.alert_name.len() > 100 {
        format!("{}...", &incident.alert_name[..97])
    } else {
        incident.alert_name.clone()
    };

    let thread = channel
        .create_thread_from_message(
            http,
            message.id,
            CreateThread::new(thread_name).auto_archive_duration(serenity::all::AutoArchiveDuration::OneDay),
        )
        .await
        .map_err(|e| AppError::Discord(format!("Failed to create thread: {}", e)))?;

    let context_msg = "Alert is firing";
    thread
        .id
        .send_message(http, CreateMessage::new().content(context_msg))
        .await
        .map_err(|e| AppError::Discord(format!("Failed to post thread message: {}", e)))?;

    Ok((
        message.id.to_string(),
        channel.to_string(),
        thread.id.to_string(),
    ))
}

pub async fn update_incident_embed(
    http: &Http,
    channel_id: &str,
    message_id: &str,
    incident: &Incident,
    lerke_url: Option<&str>,
) -> Result<(), AppError> {
    let channel = channel_id
        .parse::<u64>()
        .map_err(|e| AppError::Discord(format!("Invalid channel ID: {}", e)))?;
    let message = message_id
        .parse::<u64>()
        .map_err(|e| AppError::Discord(format!("Invalid message ID: {}", e)))?;

    let embed = build_embed(incident, lerke_url);

    ChannelId::new(channel)
        .edit_message(http, MessageId::new(message), EditMessage::new().embed(embed))
        .await
        .map_err(|e| AppError::Discord(format!("Failed to edit message: {}", e)))?;

    Ok(())
}

pub async fn post_thread_update(
    http: &Http,
    thread_id: &str,
    message: &str,
) -> Result<(), AppError> {
    let thread = thread_id
        .parse::<u64>()
        .map_err(|e| AppError::Discord(format!("Invalid thread ID: {}", e)))?;

    ChannelId::new(thread)
        .send_message(http, CreateMessage::new().content(message))
        .await
        .map_err(|e| AppError::Discord(format!("Failed to post thread update: {}", e)))?;

    Ok(())
}
