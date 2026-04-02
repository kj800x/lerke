use actix_web::{get, web};
use maud::{html, Markup, DOCTYPE};

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

fn pretty_json(raw: &str) -> String {
    serde_json::from_str::<serde_json::Value>(raw)
        .and_then(|v| serde_json::to_string_pretty(&v))
        .unwrap_or_else(|_| raw.to_string())
}
