use actix_web::{get, web};
use maud::{html, Markup, DOCTYPE};

use crate::db::models::{AppState, Incident};
use crate::discord::notifier::{url_key_to_label, FILTERED_LABELS};
use crate::db::queries;
use crate::error::AppResult;
use crate::web::formatting::format_eastern;
use crate::web::header;

const PROMOTED_ANNOTATIONS: &[&str] = &["summary", "description"];

fn get_annotation(incident: &Incident, key: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(&incident.annotations_json)
        .ok()
        .and_then(|v| v.get(key)?.as_str().map(|s| s.to_string()))
}

fn collect_metadata(incident: &Incident) -> Vec<(String, String)> {
    let mut pairs = Vec::new();

    if let Ok(labels) = serde_json::from_str::<serde_json::Value>(&incident.labels_json) {
        if let Some(obj) = labels.as_object() {
            for (key, value) in obj {
                if FILTERED_LABELS.contains(&key.as_str()) {
                    continue;
                }
                pairs.push((key.clone(), value.as_str().unwrap_or(&value.to_string()).to_string()));
            }
        }
    }

    if let Ok(annotations) = serde_json::from_str::<serde_json::Value>(&incident.annotations_json) {
        if let Some(obj) = annotations.as_object() {
            for (key, value) in obj {
                if PROMOTED_ANNOTATIONS.contains(&key.as_str()) || key.ends_with("_url") {
                    continue;
                }
                pairs.push((key.clone(), value.as_str().unwrap_or(&value.to_string()).to_string()));
            }
        }
    }

    pairs
}

/// Collect annotations ending in _url as (label, url) pairs
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

#[get("/incidents/{id}")]
pub async fn incident_detail_page(
    state: web::Data<AppState>,
    path: web::Path<i64>,
) -> AppResult<Markup> {
    let id = path.into_inner();
    let incident = queries::get_incident(&state.db, id)
        .await?
        .ok_or_else(|| crate::error::AppError::NotFound(format!("Incident {} not found", id)))?;

    let summary = get_annotation(&incident, "summary");
    let description = get_annotation(&incident, "description");
    let metadata = collect_metadata(&incident);
    let extra_links = collect_url_annotations(&incident);

    Ok(html! {
        (DOCTYPE)
        html {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Lerke - " (incident.alert_name) }
                (header::stylesheet_link())
                (header::scripts())
            }
            body {
                (header::render("incidents"))
                main class="container" {
                    a href="/incidents" class="back-link" { "← Back" }

                    div class="detail-header" {
                        div class="detail-title-row" {
                            h1 { (incident.alert_name) }
                            span class=(format!("status-badge {}", incident.status)) {
                                (incident.status.to_uppercase())
                            }
                        }
                        @if let Some(ref s) = summary {
                            p class="detail-summary" { (s) }
                        }
                        @if let Some(ref d) = description {
                            p class="detail-description" { (d) }
                        }
                        div class="detail-meta-line" {
                            span { "Fired " (format_eastern(&incident.first_firing_at)) }
                            @if let Some(ref resolved_at) = incident.resolved_at {
                                span { " · Resolved " (format_eastern(resolved_at)) }
                            }
                            @if let Some(ref severity) = incident.severity {
                                span { " · Severity: " (severity) }
                            }
                        }
                        div class="detail-meta-line" {
                            @for (label, url) in &extra_links {
                                a href=(url) target="_blank" { (label) }
                            }
                            @if let Some(ref url) = incident.grafana_dashboard_url {
                                a href=(url) target="_blank" { "Dashboard" }
                            }
                            @if let Some(ref url) = incident.grafana_panel_url {
                                a href=(url) target="_blank" { "Panel" }
                            }
                            @if let Some(ref url) = incident.grafana_generator_url {
                                a href=(url) target="_blank" { "Alert definition" }
                            }
                        }
                    }

                    @if !metadata.is_empty() {
                        div class="incident-section" {
                            h2 { "Metadata" }
                            div class="kv-table" {
                                @for (key, value) in &metadata {
                                    div class="kv-row" {
                                        span class="kv-key" { (key) }
                                        span class="kv-value" { (value) }
                                    }
                                }
                            }
                        }
                    }

                    div class="incident-section" {
                        h2 { "Events" }
                        div hx-get=(format!("/incidents/{}/events-fragment", incident.id))
                            hx-trigger="load, every 5s"
                            hx-swap="morph:innerHTML"
                            hx-ext="morph" {
                        }
                    }
                }
            }
        }
    })
}

#[get("/incidents/{id}/events-fragment")]
pub async fn incident_events_fragment(
    state: web::Data<AppState>,
    path: web::Path<i64>,
) -> AppResult<Markup> {
    let id = path.into_inner();
    let events = queries::list_incident_events(&state.db, id).await?;

    Ok(html! {
        @if events.is_empty() {
            div class="empty-state" {
                p { "No events recorded yet." }
            }
        } @else {
            div class="events-timeline" {
                @for event in &events {
                    div class=(format!("event-item {}", event.event_type)) {
                        div class="event-header" {
                            span class="event-type" { (event.event_type) }
                            span class="event-time" { (format_eastern(&event.created_at)) }
                        }
                        div class="event-message" { (event.message) }
                    }
                }
            }
        }
    })
}
