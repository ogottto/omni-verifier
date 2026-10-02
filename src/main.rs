use actix_cors::Cors;
use actix_web::{App, HttpServer, middleware::Logger, web};
use omni_verifier::{config::AppConfig, routes, services::verification_engine::VerificationEngine};
use std::sync::Arc;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // 1. Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,omni_verifier=debug,actix_web=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // 2. Load configuration
    let config = Arc::new(AppConfig::from_env());
    let bind_addr = format!("{}:{}", config.host, config.port);
    let payload_limit = config.max_payload_size_mb * 1024 * 1024;

    info!(
        version = env!("CARGO_PKG_VERSION"),
        addr = %bind_addr,
        port = config.port,
        max_payload_mb = config.max_payload_size_mb,
        "Starting OmniVerifier Multimodal Verification Service"
    );

    // 3. Initialize VerificationEngine with shared configuration
    let engine = Arc::new(VerificationEngine::with_config(config.clone()));
    let engine_data = web::Data::new(engine);

    // 4. Start HTTP Server with robust CORS for all origins, headers & file streaming
    HttpServer::new(move || {
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header()
            .expose_any_header()
            .max_age(3600);

        App::new()
            .wrap(cors)
            .wrap(Logger::default())
            .app_data(engine_data.clone())
            .app_data(web::JsonConfig::default().limit(payload_limit))
            .app_data(web::PayloadConfig::default().limit(payload_limit))
            .configure(routes::configure)
    })
    .bind(&bind_addr)?
    .run()
    .await
}
