use actix_web::{get, web, Responder};
use maud::{html, Markup, DOCTYPE};

use crate::db::models::AppState;
use crate::db::queries;
use crate::error::AppResult;
use crate::web::header;

#[derive(serde::Deserialize)]
pub struct IncidentFilter {
    pub status: Option<String>,
}

#[get("/incidents")]
pub async fn incidents_page(
    state: web::Data<AppState>,
    query: web::Query<IncidentFilter>,
) -> AppResult<Markup> {
    let status_filter = query.status.as_deref().and_then(|s| {
        if s == "all" || s.is_empty() {
            None
        } else {
            Some(s)
        }
    });

    let _ = &state; // state available for future use

    Ok(html! {
        (DOCTYPE)
        html {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Lerke - Incidents" }
                (header::stylesheet_link())
                (header::scripts())
            }
            body {
                (header::render("incidents"))
                main class="container" {
                    div class="page-header" {
                        h1 { "Incidents" }
                        div class="filter-bar" {
                            a href="/incidents" class=(if status_filter.is_none() { "filter-btn active" } else { "filter-btn" }) { "All" }
                            a href="/incidents?status=firing" class=(if status_filter == Some("firing") { "filter-btn active" } else { "filter-btn" }) { "Firing" }
                            a href="/incidents?status=resolved" class=(if status_filter == Some("resolved") { "filter-btn active" } else { "filter-btn" }) { "Resolved" }
                        }
                    }
                    div hx-get=(format!("/incidents-fragment{}", match status_filter {
                        Some(s) => format!("?status={}", s),
                        None => String::new(),
                    }))
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

#[get("/incidents-fragment")]
pub async fn incidents_fragment(
    state: web::Data<AppState>,
    query: web::Query<IncidentFilter>,
) -> AppResult<Markup> {
    let status_filter = query.status.as_deref().and_then(|s| {
        if s == "all" || s.is_empty() {
            None
        } else {
            Some(s)
        }
    });

    let incidents = queries::list_incidents(&state.db, status_filter).await?;

    Ok(html! {
        @if incidents.is_empty() {
            div class="empty-state" {
                p { "No incidents found." }
            }
        } @else {
            table class="incidents-table" {
                thead {
                    tr {
                        th { "Status" }
                        th { "Alert" }
                        th { "Since" }
                        th { "Last Update" }
                    }
                }
                tbody {
                    @for incident in &incidents {
                        tr class=(format!("incident-row {}", incident.status))
                           onclick=(format!("window.location='/incidents/{}'", incident.id)) {
                            td {
                                span class=(format!("status-dot {}", incident.status)) {}
                            }
                            td class="alert-name" { (incident.alert_name) }
                            td class="timestamp" { (incident.first_firing_at) }
                            td class="timestamp" { (incident.last_status_change_at) }
                        }
                    }
                }
            }
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
