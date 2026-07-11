use std::{env, net::SocketAddr, path::PathBuf, str::FromStr, sync::Arc};

use anyhow::{Context, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthMode {
    SingleUser,
    ReverseProxy,
    Disabled,
}

impl FromStr for AuthMode {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "single_user" => Ok(Self::SingleUser),
            "reverse_proxy" => Ok(Self::ReverseProxy),
            "disabled" => Ok(Self::Disabled),
            other => anyhow::bail!("unsupported REVTERN_AUTH_MODE: {other}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub bind: SocketAddr,
    pub base_url: String,
    pub auth_mode: AuthMode,
    pub environment: String,
    pub secret_key: String,
    pub web_dist: Option<PathBuf>,
}

impl Config {
    pub fn from_env() -> Result<Arc<Self>> {
        let database_url = env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://revtern:revtern@localhost:5432/revtern".to_string());
        let bind = env::var("REVTERN_BIND")
            .unwrap_or_else(|_| "0.0.0.0:3000".to_string())
            .parse()
            .context("REVTERN_BIND must be a socket address")?;
        let base_url =
            env::var("REVTERN_BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());
        let environment = env::var("REVTERN_ENV").unwrap_or_else(|_| "development".to_string());
        let auth_mode = env::var("REVTERN_AUTH_MODE")
            .unwrap_or_else(|_| "single_user".to_string())
            .parse()?;
        let secret_key = env::var("REVTERN_SECRET_KEY")
            .unwrap_or_else(|_| "dev-only-revtern-secret-change-before-production".to_string());
        let web_dist = env::var("REVTERN_WEB_DIST")
            .ok()
            .filter(|v| !v.is_empty())
            .map(PathBuf::from);

        if auth_mode == AuthMode::Disabled
            && environment != "development"
            && env::var("REVTERN_UNSAFE_DISABLE_AUTH").ok().as_deref() != Some("1")
        {
            anyhow::bail!(
                "REVTERN_AUTH_MODE=disabled is only allowed in development or with REVTERN_UNSAFE_DISABLE_AUTH=1"
            );
        }
        if environment != "development"
            && (secret_key.len() < 32
                || secret_key.contains("change-before-production")
                || secret_key.contains("change-this"))
        {
            anyhow::bail!(
                "REVTERN_SECRET_KEY must be a unique secret of at least 32 characters outside development"
            );
        }
        if environment != "development" && !base_url.starts_with("https://") {
            tracing::warn!(
                base_url,
                "REVTERN_BASE_URL is not HTTPS; terminate TLS at a trusted reverse proxy before receiving production webhooks"
            );
        }

        Ok(Arc::new(Self {
            database_url,
            bind,
            base_url,
            auth_mode,
            environment,
            secret_key,
            web_dist,
        }))
    }

    pub fn cookie_secure(&self) -> bool {
        self.base_url.starts_with("https://")
    }
}
