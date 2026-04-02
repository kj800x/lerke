use std::collections::BTreeMap;

use actix_web::{get, web};
use maud::{html, Markup, DOCTYPE};

use crate::db::models::{AppState, Incident};
use crate::db::queries;
use crate::error::AppResult;
use crate::web::formatting::{format_date_eastern, format_time_eastern};
use crate::discord::notifier::url_key_to_label;
use crate::web::header;
use crate::web::incidents::render_labels;

fn incident_date_eastern(incident: &Incident) -> String {
    format_date_eastern(&incident.first_firing_at)
}

/// Group incidents by date (Eastern time), most recent first
fn group_by_date(incidents: &[Incident]) -> Vec<(String, Vec<&Incident>)> {
    let mut groups: BTreeMap<String, Vec<&Incident>> = BTreeMap::new();
    for incident in incidents {
        let date = incident_date_eastern(incident);
        groups.entry(date).or_default().push(incident);
    }
    // Reverse to get most recent date first
    let mut result: Vec<_> = groups.into_iter().collect();
    result.reverse();
    result
}

#[get("/history")]
pub async fn history_page(_state: web::Data<AppState>) -> AppResult<Markup> {
    Ok(html! {
        (DOCTYPE)
        html {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Lerke - History" }
                (header::stylesheet_link())
                (header::scripts())
            }
            body {
                (header::render("history"))
                main class="container" {
                    div class="page-header" {
                        h1 { "Incident History" }
                    }
                    div hx-get="/history-fragment"
                        hx-trigger="load, every 30s"
                        hx-swap="morph:innerHTML"
                        hx-ext="morph" {
                    }
                }
            }
        }
    })
}

#[get("/history-fragment")]
pub async fn history_fragment(state: web::Data<AppState>) -> AppResult<Markup> {
    let incidents = queries::list_incidents(&state.db, None).await?;

    if incidents.is_empty() {
        return Ok(html! {
            div class="empty-state" {
                p { "No incidents recorded yet." }
            }
        });
    }

    let grouped = group_by_date(&incidents);

    Ok(html! {
        @for (date, day_incidents) in &grouped {
            div class="history-day" {
                h2 class="history-date" { (date) }
                div class="history-cards" {
                    @for incident in day_incidents {
                        div class=(format!("history-card {}", incident.status))
                           onclick=(format!("window.location='/incidents/{}'", incident.id)) {
                            div class="history-card-header" {
                                span class=(format!("status-dot {}", incident.status)) {}
                                span class="history-card-name" { (incident.alert_name) }
                                span class=(format!("status-badge small {}", incident.status)) {
                                    (incident.status.to_uppercase())
                                }
                            }
                            div class="history-card-meta" {
                                span class="history-time" {
                                    "Fired " (format_time_eastern(&incident.first_firing_at))
                                }
                                @if let Some(ref resolved_at) = incident.resolved_at {
                                    span class="history-time" {
                                        " · Resolved " (format_time_eastern(resolved_at))
                                    }
                                }
                            }
                            (render_labels(incident))
                            div class="history-card-links" {
                                @if let Ok(annotations) = serde_json::from_str::<serde_json::Value>(&incident.annotations_json) {
                                    @if let Some(obj) = annotations.as_object() {
                                        @for (key, value) in obj {
                                            @if let Some(prefix) = key.strip_suffix("_url") {
                                                @if let Some(url) = value.as_str() {
                                                    a href=(url) target="_blank" onclick="event.stopPropagation()" class="inline-link" { (url_key_to_label(prefix)) }
                                                }
                                            }
                                        }
                                    }
                                }
                                @if let Some(ref url) = incident.grafana_dashboard_url {
                                    a href=(url) target="_blank" onclick="event.stopPropagation()" class="inline-link" { "Dashboard" }
                                }
                                @if let Some(ref url) = incident.grafana_panel_url {
                                    a href=(url) target="_blank" onclick="event.stopPropagation()" class="inline-link" { "Panel" }
                                }
                            }
                        }
                    }
                }
            }
        }
    })
}
