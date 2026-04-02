mod config;
mod db;
mod deadman;
mod discord;
mod error;
mod metrics;
mod prelude;
mod web;
mod webhooks;

use std::sync::Arc;

use crate::prelude::*;
use lerke::serve_static_file;

async fn start_http(
    registry: prometheus::Registry,
    state: db::models::AppState,
    bind_address: String,
) -> Result<(), std::io::Error> {
    log::info!("Starting HTTP server at http://{}", bind_address);

    HttpServer::new(move || {
        App::new()
            .wrap(RequestTracing::new())
            .wrap(RequestMetrics::default())
            .route(
                "/api/metrics",
                web_get().to(PrometheusMetricsHandler::new(registry.clone())),
            )
            .wrap(middleware::Logger::default())
            .app_data(Data::new(state.clone()))
            .service(web::root)
            .service(web::incidents_page)
            .service(web::incidents_fragment)
            .service(web::incident_detail_page)
            .service(web::incident_events_fragment)
            .service(web::history_page)
            .service(web::history_fragment)
            .service(web::debug_webhooks_page)
            .service(web::debug_webhooks_fragment)
            .service(web::debug_purge)
            .service(webhooks::grafana::grafana_webhook)
            .service(webhooks::uptime_kuma::uptime_kuma_webhook)
            .service(serve_static_file!("htmx.min.js"))
            .service(serve_static_file!("idiomorph.min.js"))
            .service(serve_static_file!("idiomorph-ext.min.js"))
            .service(serve_static_file!("styles.css"))
    })
    .bind(&bind_address)?
    .run()
    .await
}

#[actix_web::main]
#[allow(clippy::expect_used)]
async fn main() -> std::io::Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .filter_module("serenity", log::LevelFilter::Warn)
        .filter_module("actix_web::middleware::logger", log::LevelFilter::Warn)
        .filter_module("lerke::discord", log::LevelFilter::Info)
        .filter_module("lerke::webhooks", log::LevelFilter::Info)
        .filter_module("lerke::web", log::LevelFilter::Info)
        .parse_default_env()
        .init();

    let config = config::Config::from_env().expect("Failed to load configuration");

    // Prometheus + OpenTelemetry setup
    let registry = prometheus::Registry::new();
    let exporter = opentelemetry_prometheus::exporter()
        .with_registry(registry.clone())
        .build()
        .expect("Failed to build OpenTelemetry Prometheus exporter");
    let provider = opentelemetry_sdk::metrics::MeterProvider::builder()
        .with_reader(exporter)
        .build();
    opentelemetry::global::set_meter_provider(provider);
    metrics::init(&registry).expect("Failed to initialize metrics");

    // Database setup
    std::fs::create_dir_all("data").expect("Failed to create data directory");

    let pool = sqlx::SqlitePool::connect(&config.database_url)
        .await
        .expect("Failed to connect to database");

    sqlx::query("PRAGMA journal_mode=WAL;")
        .execute(&pool)
        .await
        .expect("Failed to set WAL journal mode");

    sqlx::migrate!()
        .run(&pool)
        .await
        .expect("Failed to run database migrations");

    // Discord HTTP client (REST only, no gateway)
    let discord_http = Arc::new(serenity::http::Http::new(&config.discord_bot_token));

    let bind_address = config.bind_address.clone();
    let deadman_url = config.uptime_kuma_push_url.clone();

    let state = db::models::AppState {
        db: pool,
        discord_http,
        config: Arc::new(config),
        webhook_log: Arc::new(tokio::sync::Mutex::new(db::models::WebhookLog::new(50))),
    };

    tokio::select! {
        _ = Box::pin(start_http(registry, state, bind_address)) => {},
        _ = Box::pin(deadman::heartbeat_loop(deadman_url)) => {},
    };

    Ok(())
}
