use actix_web::{get, web};
use maud::{html, Markup, DOCTYPE};

use crate::db::models::AppState;
use crate::db::queries;
use crate::error::AppResult;
use crate::web::header;

#[get("/incidents/{id}")]
pub async fn incident_detail_page(
    state: web::Data<AppState>,
    path: web::Path<i64>,
) -> AppResult<Markup> {
    let id = path.into_inner();
    let incident = queries::get_incident(&state.db, id)
        .await?
        .ok_or_else(|| crate::error::AppError::NotFound(format!("Incident {} not found", id)))?;

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
                    div class="page-header" {
                        a href="/incidents" class="back-link" { "← Back" }
                        h1 { (incident.alert_name) }
                        span class=(format!("status-badge {}", incident.status)) {
                            (incident.status.to_uppercase())
                        }
                    }

                    div class="incident-details" {
                        div class="detail-grid" {
                            div class="detail-item" {
                                label { "First Firing" }
                                span { (incident.first_firing_at) }
                            }
                            div class="detail-item" {
                                label { "Last Update" }
                                span { (incident.last_status_change_at) }
                            }
                            @if let Some(ref resolved_at) = incident.resolved_at {
                                div class="detail-item" {
                                    label { "Resolved At" }
                                    span { (resolved_at) }
                                }
                            }
                            @if let Some(ref severity) = incident.severity {
                                div class="detail-item" {
                                    label { "Severity" }
                                    span { (severity) }
                                }
                            }
                        }

                        div class="detail-links" {
                            @if let Some(ref url) = incident.grafana_dashboard_url {
                                a href=(url) target="_blank" class="link-btn" { "Dashboard" }
                            }
                            @if let Some(ref url) = incident.grafana_panel_url {
                                a href=(url) target="_blank" class="link-btn" { "Panel" }
                            }
                            @if let Some(ref url) = incident.grafana_silence_url {
                                a href=(url) target="_blank" class="link-btn" { "Silence" }
                            }
                        }
                    }

                    h2 { "Events" }
                    div hx-get=(format!("/incidents/{}/events-fragment", incident.id))
                        hx-trigger="load, every 5s"
                        hx-swap="morph:innerHTML"
                        hx-ext="morph" {
                        // Fragment loads here
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
                            span class="event-time" { (event.created_at) }
                        }
                        div class="event-message" { (event.message) }
                    }
                }
            }
        }
    })
}
