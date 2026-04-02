use actix_web::{delete, post, put, web, HttpResponse};
use serde::{Deserialize, Serialize};
use serenity::all::{
    ChannelId, CreateEmbed, CreateMessage, CreateThread, EditMessage, MessageId,
};

use crate::db::models::AppState;
use crate::error::AppError;

const DEFAULT_COLOR: u32 = 0x6e7681; // grey

fn parse_color(color: Option<&str>) -> u32 {
    match color {
        Some(c) => {
            let c = c.trim_start_matches('#');
            u32::from_str_radix(c, 16).unwrap_or(DEFAULT_COLOR)
        }
        None => DEFAULT_COLOR,
    }
}

// --- Create Message ---

#[derive(Deserialize)]
pub struct CreateMessageRequest {
    pub system: String,
    pub text: String,
    pub color: Option<String>,
}

#[derive(Serialize)]
pub struct CreateMessageResponse {
    pub message_id: String,
}

#[post("/api/messages")]
pub async fn create_message(
    state: web::Data<AppState>,
    body: web::Json<CreateMessageRequest>,
) -> Result<HttpResponse, AppError> {
    let channel = ChannelId::new(state.config.discord_channel_id);
    let color = parse_color(body.color.as_deref());

    let embed = CreateEmbed::new()
        .title(&body.system)
        .description(&body.text)
        .color(color);

    let msg = channel
        .send_message(&*state.discord_http, CreateMessage::new().embed(embed))
        .await
        .map_err(|e| AppError::Discord(format!("Failed to send message: {}", e)))?;

    Ok(HttpResponse::Ok().json(CreateMessageResponse {
        message_id: msg.id.to_string(),
    }))
}

// --- Update Message ---

#[derive(Deserialize)]
pub struct UpdateMessageRequest {
    pub system: String,
    pub text: String,
    pub color: Option<String>,
}

#[put("/api/messages/{message_id}")]
pub async fn update_message(
    state: web::Data<AppState>,
    path: web::Path<String>,
    body: web::Json<UpdateMessageRequest>,
) -> Result<HttpResponse, AppError> {
    let channel = ChannelId::new(state.config.discord_channel_id);
    let message_id: u64 = path
        .into_inner()
        .parse()
        .map_err(|e| AppError::Discord(format!("Invalid message ID: {}", e)))?;
    let color = parse_color(body.color.as_deref());

    let embed = CreateEmbed::new()
        .title(&body.system)
        .description(&body.text)
        .color(color);

    channel
        .edit_message(
            &*state.discord_http,
            MessageId::new(message_id),
            EditMessage::new().embed(embed),
        )
        .await
        .map_err(|e| AppError::Discord(format!("Failed to edit message: {}", e)))?;

    Ok(HttpResponse::Ok().json(serde_json::json!({ "ok": true })))
}

// --- Reply (threaded) ---

#[derive(Deserialize)]
pub struct ReplyRequest {
    pub text: String,
}

#[derive(Serialize)]
pub struct ReplyResponse {
    pub message_id: String,
    pub thread_id: String,
}

#[post("/api/messages/{message_id}/reply")]
pub async fn reply_to_message(
    state: web::Data<AppState>,
    path: web::Path<String>,
    body: web::Json<ReplyRequest>,
) -> Result<HttpResponse, AppError> {
    let channel = ChannelId::new(state.config.discord_channel_id);
    let message_id: u64 = path
        .into_inner()
        .parse()
        .map_err(|e| AppError::Discord(format!("Invalid message ID: {}", e)))?;
    let msg_id = MessageId::new(message_id);

    // A thread created from a message shares its ID.
    // Try sending to that thread first; if it doesn't exist, create it.
    let thread_channel = ChannelId::new(message_id);
    let thread_id = match thread_channel
        .send_message(
            &*state.discord_http,
            CreateMessage::new().content(&body.text),
        )
        .await
    {
        Ok(reply) => {
            return Ok(HttpResponse::Ok().json(ReplyResponse {
                message_id: reply.id.to_string(),
                thread_id: thread_channel.to_string(),
            }));
        }
        Err(_) => {
            // Thread doesn't exist yet, create it
            let thread = channel
                .create_thread_from_message(
                    &*state.discord_http,
                    msg_id,
                    CreateThread::new("Thread")
                        .auto_archive_duration(serenity::all::AutoArchiveDuration::OneDay),
                )
                .await
                .map_err(|e| AppError::Discord(format!("Failed to create thread: {}", e)))?;
            thread.id
        }
    };

    let reply = thread_id
        .send_message(
            &*state.discord_http,
            CreateMessage::new().content(&body.text),
        )
        .await
        .map_err(|e| AppError::Discord(format!("Failed to send reply: {}", e)))?;

    Ok(HttpResponse::Ok().json(ReplyResponse {
        message_id: reply.id.to_string(),
        thread_id: thread_id.to_string(),
    }))
}

// --- Delete Message ---

#[delete("/api/messages/{message_id}")]
pub async fn delete_message(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let channel = ChannelId::new(state.config.discord_channel_id);
    let message_id: u64 = path
        .into_inner()
        .parse()
        .map_err(|e| AppError::Discord(format!("Invalid message ID: {}", e)))?;

    channel
        .delete_message(&*state.discord_http, MessageId::new(message_id))
        .await
        .map_err(|e| AppError::Discord(format!("Failed to delete message: {}", e)))?;

    Ok(HttpResponse::Ok().json(serde_json::json!({ "ok": true })))
}

