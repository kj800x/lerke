pub use actix_web::{
    middleware,
    web::{get as web_get, Data},
    App, HttpServer,
};
pub use actix_web_opentelemetry::{PrometheusMetricsHandler, RequestMetrics, RequestTracing};
