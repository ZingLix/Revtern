mod auth;
mod catchup;
mod config;
mod crypto;
mod error;
mod routes;

use std::sync::Arc;

use axum::{
    Router,
    http::{
        HeaderName, Method,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
    routing::get,
};
use config::Config;
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

    let mut app = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .merge(routes::router(state.clone()))
        .layer(
            CorsLayer::new()
                .allow_origin(AllowOrigin::mirror_request())
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

    if let Some(web_dist) = &config.web_dist {
        if web_dist.exists() {
            app = app.fallback_service(
                ServeDir::new(web_dist)
                    .not_found_service(ServeFile::new(web_dist.join("index.html"))),
            );
        }
    }

    tracing::info!(bind = %config.bind, "starting revtern api");
    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
