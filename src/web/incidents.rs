use actix_web::{get, web, Responder};
use maud::{html, Markup, DOCTYPE};

use crate::db::models::{AppState, Incident};
use crate::db::queries;
use crate::discord::notifier::FILTERED_LABELS;
use crate::error::AppResult;
use crate::web::formatting::format_eastern;
use crate::web::header;

pub fn render_labels(incident: &Incident) -> Markup {
    html! {
        @if let Ok(labels) = serde_json::from_str::<serde_json::Value>(&incident.labels_json) {
            @if let Some(obj) = labels.as_object() {
                div class="label-tags" {
                    @for (key, value) in obj {
                        @if !FILTERED_LABELS.contains(&key.as_str()) {
                            span class="label-tag" {
                                span class="label-key" { (key) }
                                "="
                                span class="label-value" { (value.as_str().unwrap_or(&value.to_string())) }
                            }
                        }
                    }
                }
            }
        }
    }
}

pub fn render_incident_table(incidents: &[Incident]) -> Markup {
    html! {
        table class="incidents-table" {
            thead {
                tr {
                    th { "Status" }
                    th { "Alert" }
                    th { "Labels" }
                    th { "Links" }
                    th { "Since" }
                    th { "Last Update" }
                }
            }
            tbody {
                @for incident in incidents {
                    tr class=(format!("incident-row {}", incident.status))
                       onclick=(format!("window.location='/incidents/{}'", incident.id)) {
                        td {
                            span class=(format!("status-dot {}", incident.status)) {}
                        }
                        td class="alert-name" { (incident.alert_name) }
                        td { (render_labels(incident)) }
                        td class="incident-links" {
                            @if let Some(ref url) = incident.grafana_dashboard_url {
                                a href=(url) target="_blank" onclick="event.stopPropagation()" class="inline-link" { "Dashboard" }
                            }
                            @if let Some(ref url) = incident.grafana_panel_url {
                                a href=(url) target="_blank" onclick="event.stopPropagation()" class="inline-link" { "Panel" }
                            }
                        }
                        td class="timestamp" { (format_eastern(&incident.first_firing_at)) }
                        td class="timestamp" { (format_eastern(&incident.last_status_change_at)) }
                    }
                }
            }
        }
    }
}

#[get("/incidents")]
pub async fn incidents_page(_state: web::Data<AppState>) -> AppResult<Markup> {
    Ok(html! {
        (DOCTYPE)
        html {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Lerke" }
                (header::stylesheet_link())
                (header::scripts())
            }
            body {
                (header::render("incidents"))
                main class="container" {
                    div hx-get="/incidents-fragment"
                        hx-trigger="load, every 5s"
                        hx-swap="morph:innerHTML"
                        hx-ext="morph" {
                    }
                }
            }
        }
    })
}

#[get("/incidents-fragment")]
pub async fn incidents_fragment(state: web::Data<AppState>) -> AppResult<Markup> {
    let incidents = queries::list_incidents(&state.db, Some("firing")).await?;

    Ok(html! {
        @if incidents.is_empty() {
            div class="all-clear" {
                div class="all-clear-icon" { "🏖️" }
                h2 { "All clear" }
                p { "No active incidents. Enjoy the calm." }
                a href="/history" class="history-link" { "Review past incidents →" }
            }
        } @else {
            div class="page-header" {
                h1 { "Active Incidents" }
                a href="/history" class="filter-btn" { "History →" }
            }
            (render_incident_table(&incidents))
        }
    })
}

impl Responder for crate::error::AppError {
    type Body = actix_web::body::BoxBody;

    fn respond_to(self, _req: &actix_web::HttpRequest) -> actix_web::HttpResponse<Self::Body> {
        use actix_web::ResponseError;
        self.error_response()
    }
}
