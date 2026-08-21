mod access;
mod auth;
mod catchup;
mod config;
mod crypto;
mod error;
mod oidc;
mod purchase_lookup;
mod routes;
mod webhook_verification;

use std::sync::Arc;

use axum::{
    Router,
    http::{
        HeaderName, HeaderValue, Method,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
    routing::get,
};
use config::Config;
use revtern_core::new_id;
use revtern_jobs::process_next_job;
use sqlx::{PgPool, postgres::PgPoolOptions};
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Arc<Config>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("revtern_api=info,tower_http=info")),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = Config::from_env()?;
    if config.auth_mode == config::AuthMode::Disabled {
        tracing::warn!("authentication is disabled; use only for local development");
    }
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect(&config.database_url)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;

    let state = AppState {
        pool,
        config: config.clone(),
    };
    let browser_origin = reqwest::Url::parse(&config.base_url)?
        .origin()
        .ascii_serialization();
    let browser_origin = HeaderValue::from_str(&browser_origin)?;

    let worker_pool = state.pool.clone();
    tokio::spawn(async move {
        let worker_id = new_id("worker");
        loop {
            match process_next_job(&worker_pool, &worker_id).await {
                Ok(true) => continue,
                Ok(false) => tokio::time::sleep(std::time::Duration::from_secs(2)).await,
                Err(error) => {
                    tracing::error!(?error, "background job failed");
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                }
            }
        }
    });

    let mut app = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .merge(routes::router(state.clone()))
        .merge(oidc::router(state.clone()))
        .layer(
            CorsLayer::new()
                .allow_origin(AllowOrigin::exact(browser_origin))
                .allow_headers([
                    CONTENT_TYPE,
                    AUTHORIZATION,
                    HeaderName::from_static("x-csrf-token"),
                    HeaderName::from_static("x-revtern-webhook-secret"),
                    HeaderName::from_static("x-revenuecat-authorization"),
                ])
                .allow_methods([
                    Method::GET,
                    Method::POST,
                    Method::PATCH,
                    Method::DELETE,
                    Method::OPTIONS,
                ])
                .allow_credentials(true),
        )
        .layer(TraceLayer::new_for_http());

    if let Some(web_dist) = &config.web_dist
        && web_dist.exists()
    {
        app = app.fallback_service(
            ServeDir::new(web_dist).not_found_service(ServeFile::new(web_dist.join("index.html"))),
        );
    }

    tracing::info!(bind = %config.bind, "starting revtern api");
    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
