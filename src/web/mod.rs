pub mod debug;
pub mod header;
pub mod incident_detail;
pub mod incidents;

pub use debug::{debug_webhooks_fragment, debug_webhooks_page};
pub use incident_detail::incident_events_fragment;
pub use incident_detail::incident_detail_page;
pub use incidents::{incidents_fragment, incidents_page};

use actix_web::{get, HttpResponse, Responder};

#[get("/")]
pub async fn root() -> impl Responder {
    HttpResponse::Found()
        .append_header(("Location", "/incidents"))
        .finish()
}
