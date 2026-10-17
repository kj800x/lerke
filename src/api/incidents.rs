//! Read-only incident API, for homelab services that decide things from
//! what is firing (the cicd deploy-checks quiet-window adapter).
use actix_web::{HttpResponse, get, web};
use serde::{Deserialize, Serialize};

use crate::db::models::{AppState, Incident};
use crate::db::queries;
use crate::error::AppError;

#[derive(Deserialize)]
pub struct IncidentsQuery {
    /// `firing` or `resolved`; absent lists every incident.
    pub status: Option<String>,
}

#[derive(Serialize)]
pub struct ApiIncident {
    pub id: i64,
    pub alert_name: String,
    pub status: String,
    pub severity: Option<String>,
    pub labels: serde_json::Value,
    pub first_firing_at: String,
    pub last_status_change_at: String,
    pub resolved_at: Option<String>,
    /// The incident's page in Lerke, when LERKE_URL is set.
    pub url: Option<String>,
}

fn to_api(incident: Incident, lerke_url: Option<&str>) -> ApiIncident {
    ApiIncident {
        url: lerke_url
            .map(|base| format!("{}/incidents/{}", base.trim_end_matches('/'), incident.id)),
        id: incident.id,
        alert_name: incident.alert_name,
        status: incident.status,
        severity: incident.severity,
        labels: serde_json::from_str(&incident.labels_json).unwrap_or(serde_json::Value::Null),
        first_firing_at: incident.first_firing_at,
        last_status_change_at: incident.last_status_change_at,
        resolved_at: incident.resolved_at,
    }
}

#[get("/api/incidents")]
pub async fn list_incidents(
    state: web::Data<AppState>,
    query: web::Query<IncidentsQuery>,
) -> Result<HttpResponse, AppError> {
    let incidents = queries::list_incidents(&state.db, query.status.as_deref()).await?;
    let lerke_url = state.config.lerke_url.as_deref();
    let body: Vec<ApiIncident> = incidents
        .into_iter()
        .map(|i| to_api(i, lerke_url))
        .collect();
    Ok(HttpResponse::Ok().json(body))
}
