use actix_web::{get, post, web, HttpResponse, Responder};
use maud::{html, Markup, DOCTYPE};
use serenity::all::ChannelId;

use crate::db::models::AppState;
use crate::web::formatting::format_eastern;
use crate::web::header;

#[get("/debug/webhooks")]
pub async fn debug_webhooks_page(_state: web::Data<AppState>) -> Markup {
    html! {
        (DOCTYPE)
        html {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Lerke - Webhook Debug" }
                (header::stylesheet_link())
                (header::scripts())
            }
            body {
                (header::render("debug"))
                main class="container" {
                    div class="page-header" {
                        h1 { "Webhook Payloads" }
                        p class="debug-hint" { "Last 50 raw payloads received. Auto-refreshes every 5s." }
                    }
                    div hx-get="/debug/webhooks-fragment"
                        hx-trigger="load, every 5s"
                        hx-swap="morph:innerHTML"
                        hx-ext="morph" {
                    }

                    div class="purge-section" {
                        h2 { "Danger Zone" }
                        p class="debug-hint" { "Purge all incidents from the database and delete all bot messages from the Discord channel." }
                        button class="purge-btn"
                            hx-post="/debug/purge"
                            hx-confirm="This will delete ALL incidents and ALL bot messages in the Discord channel. Are you sure?"
                            hx-target="#purge-result"
                            hx-swap="innerHTML" {
                            "Purge All Data"
                        }
                        div id="purge-result" {}
                    }
                }
            }
        }
    }
}

#[get("/debug/webhooks-fragment")]
pub async fn debug_webhooks_fragment(state: web::Data<AppState>) -> Markup {
    let log = state.webhook_log.lock().await;
    let entries = log.entries();

    html! {
        @if entries.is_empty() {
            div class="empty-state" {
                p { "No webhooks received yet." }
            }
        } @else {
            @for (i, entry) in entries.iter().enumerate() {
                div class="webhook-entry" {
                    div class="webhook-header" {
                        span class="webhook-index" { "#" (entries.len() - i) }
                        span class="webhook-time" { (format_eastern(&entry.received_at)) }
                    }
                    pre class="webhook-body" {
                        code { (pretty_json(&entry.raw_body)) }
                    }
                }
            }
        }
    }
}

#[post("/debug/purge")]
pub async fn debug_purge(state: web::Data<AppState>) -> impl Responder {
    let mut errors = Vec::new();

    // Delete bot messages from Discord channel
    let channel_id = ChannelId::new(state.config.discord_channel_id);
    match delete_bot_messages(&state, channel_id).await {
        Ok(count) => log::info!("Deleted {} Discord messages", count),
        Err(e) => {
            log::error!("Failed to delete Discord messages: {}", e);
            errors.push(format!("Discord: {}", e));
        }
    }

    // Clear the database
    match purge_database(&state).await {
        Ok(_) => log::info!("Purged all incident data from database"),
        Err(e) => {
            log::error!("Failed to purge database: {}", e);
            errors.push(format!("Database: {}", e));
        }
    }

    // Clear webhook log
    {
        let mut log = state.webhook_log.lock().await;
        *log = crate::db::models::WebhookLog::new(50);
    }

    if errors.is_empty() {
        HttpResponse::Ok().body("<span class=\"purge-success\">All data purged successfully.</span>")
    } else {
        HttpResponse::Ok().body(format!(
            "<span class=\"purge-error\">Partial purge. Errors: {}</span>",
            errors.join("; ")
        ))
    }
}

async fn delete_bot_messages(
    state: &AppState,
    channel_id: ChannelId,
) -> Result<usize, crate::error::AppError> {
    let http = &state.discord_http;
    let mut deleted = 0;

    // Get the bot's own user ID
    let bot_user = http
        .get_current_user()
        .await
        .map_err(|e| crate::error::AppError::Discord(format!("Failed to get bot user: {}", e)))?;
    let bot_id = bot_user.id;

    // Fetch and delete messages in batches
    let mut last_message_id = None;
    loop {
        let mut builder = serenity::all::GetMessages::new().limit(100);
        if let Some(before) = last_message_id {
            builder = builder.before(before);
        }

        let messages = channel_id
            .messages(http, builder)
            .await
            .map_err(|e| crate::error::AppError::Discord(format!("Failed to fetch messages: {}", e)))?;

        if messages.is_empty() {
            break;
        }

        last_message_id = messages.last().map(|m| m.id);

        for msg in &messages {
            if msg.author.id == bot_id {
                // Also delete any threads created from this message
                if let Err(e) = channel_id
                    .delete_message(http, msg.id)
                    .await
                {
                    log::warn!("Failed to delete message {}: {}", msg.id, e);
                } else {
                    deleted += 1;
                }
            }
        }

        // If we got fewer than 100, we've reached the end
        if messages.len() < 100 {
            break;
        }
    }

    Ok(deleted)
}

async fn purge_database(state: &AppState) -> Result<(), crate::error::AppError> {
    sqlx::query("DELETE FROM incident_events")
        .execute(&state.db)
        .await?;
    sqlx::query("DELETE FROM incidents")
        .execute(&state.db)
        .await?;
    Ok(())
}

fn pretty_json(raw: &str) -> String {
    serde_json::from_str::<serde_json::Value>(raw)
        .and_then(|v| serde_json::to_string_pretty(&v))
        .unwrap_or_else(|_| raw.to_string())
}
