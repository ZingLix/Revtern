use std::sync::Arc;

use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use axum::{
    extract::FromRequestParts,
    http::{HeaderMap, Method, request::Parts},
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use rand::RngCore;
use revtern_core::{new_id, sha256_hex};
use serde::Serialize;
use sqlx::Row;
use time::{Duration, OffsetDateTime};

use crate::{
    AppState,
    config::{AuthMode, Config},
    error::ApiError,
};

pub const SESSION_COOKIE: &str = "revtern_session";
pub const CSRF_COOKIE: &str = "revtern_csrf";

#[derive(Debug, Clone, Serialize)]
pub struct CurrentUser {
    pub user: UserSummary,
    pub workspace: WorkspaceSummary,
}

#[derive(Debug, Clone, Serialize)]
pub struct UserSummary {
    pub id: String,
    pub email: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceSummary {
    pub id: String,
    pub name: String,
}

pub struct CsrfGuard;

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = ApiError;

    fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        let headers = parts.headers.clone();
        let pool = state.pool.clone();
        let mode = state.config.auth_mode.clone();
        async move {
            match mode {
                AuthMode::SingleUser => current_from_session(&pool, &headers).await,
                AuthMode::ReverseProxy => current_from_reverse_proxy(&pool, &headers).await,
                AuthMode::Disabled => current_disabled(&pool).await,
            }
        }
    }
}

impl FromRequestParts<AppState> for CsrfGuard {
    type Rejection = ApiError;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &AppState,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        let method = parts.method.clone();
        let headers = parts.headers.clone();
        async move {
            if matches!(method, Method::GET | Method::HEAD | Method::OPTIONS) {
                return Ok(Self);
            }
            let jar = CookieJar::from_headers(&headers);
            let cookie = jar
                .get(CSRF_COOKIE)
                .map(|cookie| cookie.value().to_string())
                .ok_or_else(|| ApiError::Forbidden("missing csrf cookie".to_string()))?;
            let header = headers
                .get("x-csrf-token")
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| ApiError::Forbidden("missing csrf header".to_string()))?;
            if cookie != header {
                return Err(ApiError::Forbidden("invalid csrf token".to_string()));
            }
            Ok(Self)
        }
    }
}

pub fn hash_password(password: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Ok(Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?
        .to_string())
}

pub fn verify_password(password: &str, password_hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(password_hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

pub async fn create_session(
    pool: &sqlx::PgPool,
    config: &Arc<Config>,
    user_id: &str,
    jar: CookieJar,
) -> Result<CookieJar, ApiError> {
    let session_id = new_id("ses");
    let token = random_token();
    let csrf = random_token();
    let session_hash = sha256_hex(token.as_bytes());
    let expires_at = OffsetDateTime::now_utc() + Duration::days(30);
    sqlx::query(
        "insert into sessions (id, user_id, session_hash, expires_at, created_at, last_seen_at) values ($1, $2, $3, $4, now(), now())",
    )
    .bind(session_id)
    .bind(user_id)
    .bind(session_hash)
    .bind(expires_at)
    .execute(pool)
    .await?;

    let session_cookie = Cookie::build((SESSION_COOKIE, token))
        .http_only(true)
        .secure(config.cookie_secure())
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(Duration::days(30))
        .build();
    let csrf_cookie = Cookie::build((CSRF_COOKIE, csrf))
        .http_only(false)
        .secure(config.cookie_secure())
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(Duration::days(30))
        .build();
    Ok(jar.add(session_cookie).add(csrf_cookie))
}

pub async fn clear_session(
    pool: &sqlx::PgPool,
    headers: &HeaderMap,
    jar: CookieJar,
) -> Result<CookieJar, ApiError> {
    if let Some(token) = CookieJar::from_headers(headers)
        .get(SESSION_COOKIE)
        .map(|cookie| cookie.value().to_string())
    {
        sqlx::query("delete from sessions where session_hash = $1")
            .bind(sha256_hex(token.as_bytes()))
            .execute(pool)
            .await?;
    }
    Ok(jar
        .remove(Cookie::from(SESSION_COOKIE))
        .remove(Cookie::from(CSRF_COOKIE)))
}

async fn current_from_session(
    pool: &sqlx::PgPool,
    headers: &HeaderMap,
) -> Result<CurrentUser, ApiError> {
    let jar = CookieJar::from_headers(headers);
    let token = jar
        .get(SESSION_COOKIE)
        .map(|cookie| cookie.value().to_string())
        .ok_or_else(|| ApiError::Unauthorized("login required".to_string()))?;
    let row = sqlx::query(
        r#"
        select u.id as user_id, u.email, wu.role, w.id as workspace_id, w.name as workspace_name
        from sessions s
        join users u on u.id = s.user_id
        join workspace_users wu on wu.user_id = u.id
        join workspaces w on w.id = wu.workspace_id
        where s.session_hash = $1 and s.expires_at > now()
        order by w.created_at asc
        limit 1
        "#,
    )
    .bind(sha256_hex(token.as_bytes()))
    .fetch_optional(pool)
    .await?;
    let row = row.ok_or_else(|| ApiError::Unauthorized("session expired".to_string()))?;
    let user_id: String = row.try_get("user_id")?;
    sqlx::query("update sessions set last_seen_at = now() where session_hash = $1")
        .bind(sha256_hex(token.as_bytes()))
        .execute(pool)
        .await?;
    Ok(CurrentUser {
        user: UserSummary {
            id: user_id,
            email: row.try_get("email")?,
            role: row.try_get("role")?,
        },
        workspace: WorkspaceSummary {
            id: row.try_get("workspace_id")?,
            name: row.try_get("workspace_name")?,
        },
    })
}

async fn current_from_reverse_proxy(
    pool: &sqlx::PgPool,
    headers: &HeaderMap,
) -> Result<CurrentUser, ApiError> {
    let email = headers
        .get("x-forwarded-email")
        .or_else(|| headers.get("x-forwarded-user"))
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::Unauthorized("reverse proxy user header missing".to_string()))?;
    let existing = sqlx::query(
        r#"
        select u.id as user_id, u.email, wu.role, w.id as workspace_id, w.name as workspace_name
        from users u
        join workspace_users wu on wu.user_id = u.id
        join workspaces w on w.id = wu.workspace_id
        where u.email = $1
        limit 1
        "#,
    )
    .bind(email)
    .fetch_optional(pool)
    .await?;
    if let Some(row) = existing {
        return Ok(CurrentUser {
            user: UserSummary {
                id: row.try_get("user_id")?,
                email: row.try_get("email")?,
                role: row.try_get("role")?,
            },
            workspace: WorkspaceSummary {
                id: row.try_get("workspace_id")?,
                name: row.try_get("workspace_name")?,
            },
        });
    }

    let workspace_id = ensure_workspace(pool).await?;
    let user_id = new_id("usr");
    sqlx::query("insert into users (id, email, password_hash, role, created_at) values ($1, $2, 'reverse_proxy', 'owner', now())")
        .bind(&user_id)
        .bind(email)
        .execute(pool)
        .await?;
    sqlx::query(
        "insert into workspace_users (workspace_id, user_id, role) values ($1, $2, 'owner')",
    )
    .bind(&workspace_id)
    .bind(&user_id)
    .execute(pool)
    .await?;
    Ok(CurrentUser {
        user: UserSummary {
            id: user_id,
            email: email.to_string(),
            role: "owner".to_string(),
        },
        workspace: WorkspaceSummary {
            id: workspace_id,
            name: "Revtern".to_string(),
        },
    })
}

async fn current_disabled(pool: &sqlx::PgPool) -> Result<CurrentUser, ApiError> {
    let row = sqlx::query(
        r#"
        select u.id as user_id, u.email, wu.role, w.id as workspace_id, w.name as workspace_name
        from users u
        join workspace_users wu on wu.user_id = u.id
        join workspaces w on w.id = wu.workspace_id
        order by u.created_at asc
        limit 1
        "#,
    )
    .fetch_optional(pool)
    .await?;
    if let Some(row) = row {
        return Ok(CurrentUser {
            user: UserSummary {
                id: row.try_get("user_id")?,
                email: row.try_get("email")?,
                role: row.try_get("role")?,
            },
            workspace: WorkspaceSummary {
                id: row.try_get("workspace_id")?,
                name: row.try_get("workspace_name")?,
            },
        });
    }

    let workspace_id = ensure_workspace(pool).await?;
    let user_id = new_id("usr");
    sqlx::query("insert into users (id, email, password_hash, role, created_at) values ($1, 'dev@revtern.local', 'disabled', 'owner', now())")
        .bind(&user_id)
        .execute(pool)
        .await?;
    sqlx::query(
        "insert into workspace_users (workspace_id, user_id, role) values ($1, $2, 'owner')",
    )
    .bind(&workspace_id)
    .bind(&user_id)
    .execute(pool)
    .await?;
    Ok(CurrentUser {
        user: UserSummary {
            id: user_id,
            email: "dev@revtern.local".to_string(),
            role: "owner".to_string(),
        },
        workspace: WorkspaceSummary {
            id: workspace_id,
            name: "Revtern".to_string(),
        },
    })
}

async fn ensure_workspace(pool: &sqlx::PgPool) -> Result<String, ApiError> {
    if let Some(row) = sqlx::query("select id from workspaces order by created_at asc limit 1")
        .fetch_optional(pool)
        .await?
    {
        return Ok(row.try_get("id")?);
    }
    let id = new_id("wsp");
    sqlx::query("insert into workspaces (id, name, created_at) values ($1, 'Revtern', now())")
        .bind(&id)
        .execute(pool)
        .await?;
    Ok(id)
}

fn random_token() -> String {
    let mut bytes = [0_u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}
