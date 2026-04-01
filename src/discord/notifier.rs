use serenity::all::{
    ChannelId, CreateEmbed, CreateMessage, CreateThread, EditMessage, MessageId,
};
use serenity::http::Http;

use crate::db::models::Incident;
use crate::error::AppError;

const COLOR_RED: u32 = 0xFF0000;
const COLOR_GREEN: u32 = 0x00CC00;

fn build_embed(incident: &Incident) -> CreateEmbed {
    let color = if incident.status == "resolved" {
        COLOR_GREEN
    } else {
        COLOR_RED
    };

    let status_text = if incident.status == "resolved" {
        "Resolved"
    } else {
        "FIRING"
    };

    let mut embed = CreateEmbed::new()
        .title(&incident.alert_name)
        .color(color)
        .field("Status", status_text, true)
        .field("Started", &incident.first_firing_at, true);

    if let Some(ref url) = incident.grafana_dashboard_url {
        embed = embed.field("Dashboard", format!("[Open in Grafana]({})", url), false);
    }

    if let Some(ref url) = incident.grafana_panel_url {
        embed = embed.field("Panel", format!("[View Panel]({})", url), false);
    }

    if let Some(ref url) = incident.grafana_silence_url {
        embed = embed.field("Silence", format!("[Silence Alert]({})", url), false);
    }

    if let Some(ref resolved_at) = incident.resolved_at {
        embed = embed.field("Resolved", resolved_at, true);
    }

    embed
}

pub async fn send_firing_notification(
    http: &Http,
    channel_id: u64,
    incident: &Incident,
) -> Result<(String, String, String), AppError> {
    let channel = ChannelId::new(channel_id);
    let embed = build_embed(incident);

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

    // Post initial context in the thread
    let context_msg = format!(
        "Alert **{}** is now firing.\nFirst detected: {}",
        incident.alert_name, incident.first_firing_at
    );
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
) -> Result<(), AppError> {
    let channel = channel_id
        .parse::<u64>()
        .map_err(|e| AppError::Discord(format!("Invalid channel ID: {}", e)))?;
    let message = message_id
        .parse::<u64>()
        .map_err(|e| AppError::Discord(format!("Invalid message ID: {}", e)))?;

    let embed = build_embed(incident);

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
