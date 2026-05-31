pub use actix_web::{
    App, HttpServer, middleware,
    web::{Data, get as web_get},
};
pub use actix_web_opentelemetry::{PrometheusMetricsHandler, RequestMetrics, RequestTracing};
