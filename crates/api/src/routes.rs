use std::collections::HashMap;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, patch, post},
};
use axum_extra::extract::CookieJar;
use revtern_connectors::extract_event;
use revtern_core::{new_id, payload_sha256, sha256_hex};
use revtern_jobs::{enqueue_normalization, process_normalization_job, process_raw_event};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{Postgres, QueryBuilder, Row};
use time::{
    Date, Duration, OffsetDateTime, format_description::well_known::Rfc3339,
    macros::format_description,
};

use crate::{
    AppState,
    access::{self, Capability},
    auth::{self, CsrfGuard, CurrentUser},
    catchup::{
        CatchUpWindow, acknowledge_batch, fetch_webhook_notifications,
        request_app_store_test_notification,
    },
    config::{AuthMode, RegistrationMode},
    crypto,
    error::{ApiError, ApiResult},
    purchase_lookup, webhook_verification,
};

pub fn router(state: AppState) -> Router {
    Router::new()
        .nest(
            "/api",
            Router::new()
                .route("/setup/status", get(setup_status))
                .route("/setup/owner", post(setup_owner))
                .route("/registration", post(register_user))
                .route(
                    "/invitations/{token}",
                    get(get_invitation).post(accept_invitation),
                )
                .route("/session", post(create_session).delete(delete_session))
                .route(
                    "/mobile/session",
                    post(create_mobile_session).delete(delete_mobile_session),
                )
                .route("/me", get(me))
                .route("/apps", get(list_apps).post(create_app))
                .route("/apps/{app_id}", patch(update_app))
                .route("/apps/{app_id}/members", get(list_app_members))
                .route("/apps/{app_id}/invitations", post(create_app_invitation))
                .route(
                    "/apps/{app_id}/invitations/{invitation_id}",
                    axum::routing::delete(revoke_app_invitation),
                )
                .route(
                    "/apps/{app_id}/members/{member_user_id}",
                    patch(update_app_member).delete(remove_app_member),
                )
                .route(
                    "/data-sources",
                    get(list_data_sources).post(create_data_source),
                )
                .route("/data-sources/{source_id}", get(get_data_source))
                .route(
                    "/data-sources/{source_id}/credentials",
                    patch(update_data_source_credentials),
                )
                .route("/data-sources/{source_id}/test", post(test_data_source))
                .route(
                    "/data-sources/{source_id}/app-store-test-notification",
                    post(send_app_store_test_notification),
                )
                .route(
                    "/data-sources/{source_id}/catch-up",
                    post(catch_up_data_source),
                )
                .route("/products/logical", get(list_logical_products))
                .route("/products/source", get(list_source_products))
                .route("/products/catalog-confirmations", post(confirm_catalog))
                .route("/events/raw", get(list_raw_events))
                .route("/events/raw/{event_id}", get(get_raw_event))
                .route("/events/normalized", get(list_normalized_events))
                .route("/events/normalized/{event_id}", get(get_normalized_event))
                .route("/transactions", get(list_transactions))
                .route("/transactions/{transaction_id}", get(get_transaction))
                .route("/subscriptions", get(list_subscriptions))
                .route("/subscriptions/{subscription_id}", get(get_subscription))
                .route("/metrics/overview", get(metrics_overview))
                .route(
                    "/metrics/revenue-timeseries",
                    get(metrics_revenue_timeseries),
                )
                .route(
                    "/metrics/subscription-timeseries",
                    get(metrics_subscription_timeseries),
                )
                .route("/metrics/breakdown", get(metrics_breakdown))
                .route("/sync-runs", get(list_sync_runs))
                .route("/sync-runs/{sync_run_id}", get(get_sync_run))
                .route("/jobs", get(list_jobs))
                .route("/jobs/{job_id}/retry", post(retry_job))
                .route("/demo/seed", post(seed_demo))
                .route("/export/transactions.csv", get(export_transactions_csv)),
        )
        .route("/readyz", get(readyz))
        .route("/webhooks/{source_type}/{source_id}", post(ingest_webhook))
        .with_state(state)
}

async fn readyz(State(state): State<AppState>) -> ApiResult<&'static str> {
    sqlx::query_scalar::<_, i32>("select 1")
        .fetch_one(&state.pool)
        .await?;
    Ok("ready")
}

#[derive(Debug, Deserialize)]
struct SetupOwnerRequest {
    email: String,
    password: String,
    workspace_name: String,
}

async fn setup_status(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let count: i64 = sqlx::query_scalar("select count(*) from users")
        .fetch_one(&state.pool)
        .await?;
    Ok(Json(json!({
        "needs_setup": count == 0 && state.config.auth_mode == AuthMode::Local,
        "auth_mode": auth_mode_name(&state.config.auth_mode),
        "registration_mode": registration_mode_name(&state.config.registration_mode),
        "oidc": state.config.oidc.as_ref().map(|provider| json!({ "name": provider.name })),
    })))
}

async fn setup_owner(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(input): Json<SetupOwnerRequest>,
) -> ApiResult<(CookieJar, Json<Value>)> {
    if state.config.auth_mode != AuthMode::Local {
        return Err(ApiError::invalid("owner setup requires local auth mode"));
    }
    let mut tx = state.pool.begin().await?;
    sqlx::query("select pg_advisory_xact_lock(hashtext('revtern_setup_owner'))")
        .execute(&mut *tx)
        .await?;
    let existing: i64 = sqlx::query_scalar("select count(*) from users")
        .fetch_one(&mut *tx)
        .await?;
    if existing > 0 {
        return Err(ApiError::Conflict(
            "owner has already been created".to_string(),
        ));
    }
    if !input.email.contains('@') {
        return Err(ApiError::invalid("email must be valid"));
    }
    if input.password.len() < 8 {
        return Err(ApiError::invalid("password must be at least 8 characters"));
    }
    if input.workspace_name.trim().is_empty() {
        return Err(ApiError::invalid("workspace_name is required"));
    }

    let workspace_id = new_id("wsp");
    let user_id = new_id("usr");
    let password_hash = auth::hash_password(&input.password)?;
    sqlx::query("insert into workspaces (id, name, created_at) values ($1, $2, now())")
        .bind(&workspace_id)
        .bind(input.workspace_name.trim())
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "insert into users (id, email, password_hash, display_name, role, status, created_at, updated_at, last_login_at) values ($1, $2, $3, $4, 'owner', 'active', now(), now(), now())",
    )
    .bind(&user_id)
    .bind(input.email.trim().to_ascii_lowercase())
    .bind(password_hash)
    .bind(input.email.trim())
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "insert into workspace_users (workspace_id, user_id, role, status, created_at, updated_at) values ($1, $2, 'owner', 'active', now(), now())",
    )
    .bind(&workspace_id)
    .bind(&user_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    let jar = auth::create_session(&state.pool, &state.config, &user_id, jar).await?;
    Ok((jar, Json(json!({ "created": true }))))
}

#[derive(Debug, Deserialize)]
struct RegistrationRequest {
    email: String,
    password: String,
    display_name: Option<String>,
    invite_token: Option<String>,
}

async fn register_user(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(input): Json<RegistrationRequest>,
) -> ApiResult<(CookieJar, Json<Value>)> {
    if state.config.auth_mode != AuthMode::Local {
        return Err(ApiError::invalid("registration requires local auth mode"));
    }
    let email = input.email.trim().to_ascii_lowercase();
    if !email.contains('@') {
        return Err(ApiError::invalid("email must be valid"));
    }
    if input.password.len() < 8 {
        return Err(ApiError::invalid("password must be at least 8 characters"));
    }
    if state.config.registration_mode == RegistrationMode::Closed {
        return Err(ApiError::Forbidden("registration is closed".to_string()));
    }
    if state.config.registration_mode == RegistrationMode::InviteOnly
        && input.invite_token.as_deref().unwrap_or_default().is_empty()
    {
        return Err(ApiError::Forbidden(
            "a valid invitation is required".to_string(),
        ));
    }

    let display_name = input
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&email)
        .to_string();
    let password_hash = auth::hash_password(&input.password)?;
    let user_id = new_id("usr");
    let workspace_id = new_id("wsp");
    let mut tx = state.pool.begin().await?;

    let existing: bool = sqlx::query_scalar("select exists(select 1 from users where email = $1)")
        .bind(&email)
        .fetch_one(&mut *tx)
        .await?;
    if existing {
        return Err(ApiError::Conflict(
            "an account with this email already exists".to_string(),
        ));
    }

    sqlx::query(
        "insert into users (id, email, password_hash, display_name, role, status, created_at, updated_at) values ($1, $2, $3, $4, 'owner', 'active', now(), now())",
    )
    .bind(&user_id)
    .bind(&email)
    .bind(password_hash)
    .bind(&display_name)
    .execute(&mut *tx)
    .await?;
    sqlx::query("insert into workspaces (id, name, created_at) values ($1, $2, now())")
        .bind(&workspace_id)
        .bind(format!("{display_name}'s Apps"))
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "insert into workspace_users (workspace_id, user_id, role, status, created_at, updated_at) values ($1, $2, 'owner', 'active', now(), now())",
    )
    .bind(&workspace_id)
    .bind(&user_id)
    .execute(&mut *tx)
    .await?;

    let accepted_app_id = if let Some(token) = input
        .invite_token
        .as_deref()
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        Some(
            accept_invitation_in_transaction(
                &mut tx,
                &sha256_hex(token.as_bytes()),
                &user_id,
                &email,
            )
            .await?,
        )
    } else {
        None
    };

    sqlx::query(
        "insert into audit_events (id, workspace_id, app_id, actor_user_id, action, target_type, target_id, metadata, created_at) values ($1, $2, $3, $4, 'user.registered', 'user', $4, $5, now())",
    )
    .bind(new_id("aud"))
    .bind(&workspace_id)
    .bind(accepted_app_id.as_deref())
    .bind(&user_id)
    .bind(json!({ "auth_method": "local" }))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    let jar = auth::create_session(&state.pool, &state.config, &user_id, jar).await?;
    Ok((
        jar,
        Json(json!({
            "created": true,
            "user_id": user_id,
            "workspace_id": workspace_id,
            "accepted_app_id": accepted_app_id
        })),
    ))
}

async fn get_invitation(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> ApiResult<Json<Value>> {
    let row = sqlx::query(
        r#"
        select ai.id, ai.email, ai.expires_at, a.id as app_id, a.name as app_name,
               ar.role_key, u.display_name as inviter_name
        from app_invitations ai
        join apps a on a.id = ai.app_id and a.deleted_at is null
        join app_roles ar on ar.id = ai.role_id
        left join users u on u.id = ai.invited_by_user_id
        where ai.token_hash = $1
          and ai.accepted_at is null
          and ai.revoked_at is null
          and ai.expires_at > now()
        "#,
    )
    .bind(sha256_hex(token.as_bytes()))
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| ApiError::NotFound("invitation not found or expired".to_string()))?;
    Ok(Json(json!({
        "invitation": {
            "id": row.try_get::<String, _>("id")?,
            "email": row.try_get::<String, _>("email")?,
            "expires_at": dt(row.try_get::<OffsetDateTime, _>("expires_at")?),
            "app_id": row.try_get::<String, _>("app_id")?,
            "app_name": row.try_get::<String, _>("app_name")?,
            "role": row.try_get::<String, _>("role_key")?,
            "inviter_name": row.try_get::<Option<String>, _>("inviter_name")?,
        }
    })))
}

async fn accept_invitation(
    State(state): State<AppState>,
    user: CurrentUser,
    _csrf: CsrfGuard,
    Path(token): Path<String>,
) -> ApiResult<Json<Value>> {
    let mut tx = state.pool.begin().await?;
    let app_id = accept_invitation_in_transaction(
        &mut tx,
        &sha256_hex(token.as_bytes()),
        &user.user.id,
        &user.user.email,
    )
    .await?;
    tx.commit().await?;
    let access = access::app_access(&state.pool, &user.user.id, &app_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("app not found".to_string()))?;
    access::audit(
        &state.pool,
        &user,
        Some(&access.workspace_id),
        Some(&app_id),
        "app.invitation.accepted",
        Some("app"),
        Some(&app_id),
        json!({}),
    )
    .await?;
    Ok(Json(json!({ "accepted": true, "app_id": app_id })))
}

pub(crate) async fn accept_invitation_in_transaction(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    token_hash: &str,
    user_id: &str,
    email: &str,
) -> ApiResult<String> {
    let row = sqlx::query(
        r#"
        select ai.id, ai.app_id, ai.normalized_email, ai.role_id, a.workspace_id
        from app_invitations ai
        join apps a on a.id = ai.app_id and a.deleted_at is null
        where ai.token_hash = $1
          and ai.accepted_at is null
          and ai.revoked_at is null
          and ai.expires_at > now()
        for update of ai
        "#,
    )
    .bind(token_hash)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| ApiError::NotFound("invitation not found or expired".to_string()))?;
    let normalized_email: String = row.try_get("normalized_email")?;
    if normalized_email != email.trim().to_ascii_lowercase() {
        return Err(ApiError::Forbidden(
            "this invitation was issued to a different email address".to_string(),
        ));
    }
    let invitation_id: String = row.try_get("id")?;
    let app_id: String = row.try_get("app_id")?;
    let workspace_id: String = row.try_get("workspace_id")?;
    let role_id: String = row.try_get("role_id")?;
    sqlx::query(
        r#"
        insert into workspace_users (workspace_id, user_id, role, status, created_at, updated_at)
        values ($1, $2, 'guest', 'active', now(), now())
        on conflict (workspace_id, user_id) do nothing
        "#,
    )
    .bind(&workspace_id)
    .bind(user_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        r#"
        insert into app_memberships (app_id, user_id, role_id, created_at, updated_at)
        values ($1, $2, $3, now(), now())
        on conflict (app_id, user_id) do update
          set role_id = excluded.role_id, updated_at = now()
        "#,
    )
    .bind(&app_id)
    .bind(user_id)
    .bind(role_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "update app_invitations set accepted_at = now(), accepted_by_user_id = $2 where id = $1",
    )
    .bind(invitation_id)
    .bind(user_id)
    .execute(&mut **tx)
    .await?;
    Ok(app_id)
}

#[derive(Debug, Deserialize)]
struct LoginRequest {
    email: String,
    password: String,
}

async fn create_session(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(input): Json<LoginRequest>,
) -> ApiResult<(CookieJar, Json<Value>)> {
    if state.config.auth_mode != AuthMode::Local {
        return Err(ApiError::invalid("password login requires local auth mode"));
    }
    let row =
        sqlx::query("select id, password_hash from users where email = $1 and status = 'active'")
            .bind(input.email.trim().to_ascii_lowercase())
            .fetch_optional(&state.pool)
            .await?;
    let row = row.ok_or_else(|| ApiError::Unauthorized("invalid email or password".to_string()))?;
    let user_id: String = row.try_get("id")?;
    let password_hash: Option<String> = row.try_get("password_hash")?;
    if !password_hash
        .as_deref()
        .is_some_and(|hash| auth::verify_password(&input.password, hash))
    {
        return Err(ApiError::Unauthorized(
            "invalid email or password".to_string(),
        ));
    }
    sqlx::query("update users set last_login_at = now() where id = $1")
        .bind(&user_id)
        .execute(&state.pool)
        .await?;
    let jar = auth::create_session(&state.pool, &state.config, &user_id, jar).await?;
    Ok((jar, Json(json!({ "logged_in": true }))))
}

async fn create_mobile_session(
    State(state): State<AppState>,
    Json(input): Json<LoginRequest>,
) -> ApiResult<Json<Value>> {
    if state.config.auth_mode != AuthMode::Local {
        return Err(ApiError::invalid(
            "mobile password login requires local auth mode",
        ));
    }
    let row =
        sqlx::query("select id, password_hash from users where email = $1 and status = 'active'")
            .bind(input.email.trim().to_ascii_lowercase())
            .fetch_optional(&state.pool)
            .await?;
    let row = row.ok_or_else(|| ApiError::Unauthorized("invalid email or password".to_string()))?;
    let user_id: String = row.try_get("id")?;
    let password_hash: Option<String> = row.try_get("password_hash")?;
    if !password_hash
        .as_deref()
        .is_some_and(|hash| auth::verify_password(&input.password, hash))
    {
        return Err(ApiError::Unauthorized(
            "invalid email or password".to_string(),
        ));
    }
    sqlx::query("update users set last_login_at = now() where id = $1")
        .bind(&user_id)
        .execute(&state.pool)
        .await?;
    let token = auth::create_bearer_session(&state.pool, &user_id).await?;
    Ok(Json(json!({
        "logged_in": true,
        "access_token": token,
        "token_type": "Bearer",
        "expires_in": 2_592_000
    })))
}

async fn delete_session(
    State(state): State<AppState>,
    user: CurrentUser,
    _csrf: CsrfGuard,
    headers: HeaderMap,
    jar: CookieJar,
) -> ApiResult<(CookieJar, Json<Value>)> {
    let _ = user;
    let jar = auth::clear_session(&state.pool, &headers, jar).await?;
    Ok((jar, Json(json!({ "logged_out": true }))))
}

async fn delete_mobile_session(
    State(state): State<AppState>,
    user: CurrentUser,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let _ = user;
    auth::clear_bearer_session(&state.pool, &headers).await?;
    Ok(Json(json!({ "logged_out": true })))
}

async fn me(user: CurrentUser) -> ApiResult<Json<Value>> {
    Ok(Json(
        json!({ "user": user.user, "workspace": user.workspace }),
    ))
}

#[derive(Debug, Deserialize)]
struct AppRequest {
    name: String,
    apple_bundle_id: Option<String>,
    google_package_name: Option<String>,
    platform_bundle_id: Option<String>,
    default_currency: Option<String>,
}

async fn list_apps(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    let rows = sqlx::query(
        r#"
        select a.id, a.workspace_id, a.owner_user_id, a.name, a.platform_bundle_id,
               a.apple_bundle_id, a.google_package_name, a.default_currency,
               a.created_at, a.updated_at,
               case
                 when a.owner_user_id = $1 then 'owner'
                 when wu.role in ('owner', 'admin') then 'workspace_admin'
                 else coalesce(ar.role_key, 'viewer')
               end as access_role,
               array_agg(distinct eap.permission order by eap.permission) as permissions
        from apps a
        join effective_app_permissions eap on eap.app_id = a.id and eap.user_id = $1
        left join workspace_users wu
          on wu.workspace_id = a.workspace_id and wu.user_id = $1 and wu.status = 'active'
        left join app_memberships am on am.app_id = a.id and am.user_id = $1
        left join app_roles ar on ar.id = am.role_id
        where a.deleted_at is null
        group by a.id, wu.role, ar.role_key
        order by a.created_at asc
        "#,
    )
    .bind(&user.user.id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(
        json!({ "apps": rows.into_iter().map(app_with_access_json).collect::<ApiResult<Vec<_>>>()? }),
    ))
}

async fn create_app(
    State(state): State<AppState>,
    user: CurrentUser,
    _csrf: CsrfGuard,
    Json(input): Json<AppRequest>,
) -> ApiResult<Json<Value>> {
    if matches!(user.user.role.as_str(), "viewer" | "guest") {
        return Err(ApiError::Forbidden(
            "workspace members with app creation access are required".to_string(),
        ));
    }
    if input.name.trim().is_empty() {
        return Err(ApiError::invalid("name is required"));
    }
    let id = new_id("app");
    sqlx::query(
        r#"
        insert into apps (
          id, workspace_id, owner_user_id, created_by_user_id, name,
          platform_bundle_id, apple_bundle_id, google_package_name, default_currency,
          created_at, updated_at
        ) values ($1, $2, $3, $3, $4, $5, $6, $7, $8, now(), now())
        "#,
    )
    .bind(&id)
    .bind(&user.workspace.id)
    .bind(&user.user.id)
    .bind(input.name.trim())
    .bind(input.platform_bundle_id.as_deref())
    .bind(input.apple_bundle_id.as_deref())
    .bind(input.google_package_name.as_deref())
    .bind(input.default_currency.as_deref())
    .execute(&state.pool)
    .await?;
    access::audit(
        &state.pool,
        &user,
        Some(&user.workspace.id),
        Some(&id),
        "app.created",
        Some("app"),
        Some(&id),
        json!({ "name": input.name.trim() }),
    )
    .await?;
    Ok(Json(
        json!({ "app": get_app_for_user_json(&state, &user.user.id, &id).await? }),
    ))
}

async fn update_app(
    State(state): State<AppState>,
    user: CurrentUser,
    _csrf: CsrfGuard,
    Path(app_id): Path<String>,
    Json(input): Json<AppRequest>,
) -> ApiResult<Json<Value>> {
    let app_access = access::require_app(&state.pool, &user, &app_id, Capability::AppWrite).await?;
    if input.name.trim().is_empty() {
        return Err(ApiError::invalid("name is required"));
    }
    sqlx::query(
        r#"
        update apps
        set name = $2,
            platform_bundle_id = $3,
            apple_bundle_id = $4,
            google_package_name = $5,
            default_currency = $6,
            updated_at = now()
        where id = $1
        "#,
    )
    .bind(&app_id)
    .bind(input.name.trim())
    .bind(input.platform_bundle_id.as_deref())
    .bind(input.apple_bundle_id.as_deref())
    .bind(input.google_package_name.as_deref())
    .bind(input.default_currency.as_deref())
    .execute(&state.pool)
    .await?;
    access::audit(
        &state.pool,
        &user,
        Some(&app_access.workspace_id),
        Some(&app_id),
        "app.updated",
        Some("app"),
        Some(&app_id),
        json!({ "name": input.name.trim() }),
    )
    .await?;
    Ok(Json(
        json!({ "app": get_app_for_user_json(&state, &user.user.id, &app_id).await? }),
    ))
}

#[derive(Debug, Deserialize)]
struct AppInvitationRequest {
    email: String,
    role: String,
}

#[derive(Debug, Deserialize)]
struct AppMemberRoleRequest {
    role: String,
}

async fn list_app_members(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(app_id): Path<String>,
) -> ApiResult<Json<Value>> {
    access::require_app(&state.pool, &user, &app_id, Capability::MembersManage).await?;
    let rows = sqlx::query(
        r#"
        with app_context as (
          select id, workspace_id, owner_user_id from apps where id = $1 and deleted_at is null
        ), candidates as (
          select a.owner_user_id as user_id, 'owner'::text as access_role, 'owner'::text as access_origin, 0 as priority
          from app_context a
          union all
          select wu.user_id, 'workspace_admin', 'workspace', 1
          from app_context a
          join workspace_users wu on wu.workspace_id = a.workspace_id
          where wu.status = 'active' and wu.role in ('owner', 'admin') and wu.user_id <> a.owner_user_id
          union all
          select am.user_id, ar.role_key, 'membership', 2
          from app_memberships am
          join app_roles ar on ar.id = am.role_id
          where am.app_id = $1
        )
        select distinct on (c.user_id)
               c.user_id, u.email, u.display_name, c.access_role, c.access_origin, c.priority
        from candidates c
        join users u on u.id = c.user_id and u.status = 'active'
        order by c.user_id, c.priority asc
        "#,
    )
    .bind(&app_id)
    .fetch_all(&state.pool)
    .await?;
    let invitations = sqlx::query(
        r#"
        select ai.id, ai.email, ar.role_key, ai.expires_at, ai.created_at
        from app_invitations ai
        join app_roles ar on ar.id = ai.role_id
        where ai.app_id = $1
          and ai.accepted_at is null
          and ai.revoked_at is null
          and ai.expires_at > now()
        order by ai.created_at desc
        "#,
    )
    .bind(&app_id)
    .fetch_all(&state.pool)
    .await?;
    let roles = sqlx::query(
        r#"
        select ar.role_key, ar.name, ar.description,
               array_agg(arp.permission order by arp.permission) as permissions
        from app_roles ar
        join app_role_permissions arp on arp.role_id = ar.id
        join apps a on a.id = $1
        where ar.workspace_id is null or ar.workspace_id = a.workspace_id
        group by ar.id
        order by case ar.role_key
          when 'viewer' then 0 when 'analyst' then 1 when 'editor' then 2 when 'manager' then 3 else 4 end
        "#,
    )
    .bind(&app_id)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(json!({
        "members": rows.into_iter().map(|row| -> ApiResult<Value> {
            Ok(json!({
                "user_id": row.try_get::<String, _>("user_id")?,
                "email": row.try_get::<String, _>("email")?,
                "display_name": row.try_get::<Option<String>, _>("display_name")?,
                "role": row.try_get::<String, _>("access_role")?,
                "access_origin": row.try_get::<String, _>("access_origin")?,
            }))
        }).collect::<ApiResult<Vec<_>>>()?,
        "invitations": invitations.into_iter().map(|row| -> ApiResult<Value> {
            Ok(json!({
                "id": row.try_get::<String, _>("id")?,
                "email": row.try_get::<String, _>("email")?,
                "role": row.try_get::<String, _>("role_key")?,
                "expires_at": dt(row.try_get::<OffsetDateTime, _>("expires_at")?),
                "created_at": dt(row.try_get::<OffsetDateTime, _>("created_at")?),
            }))
        }).collect::<ApiResult<Vec<_>>>()?,
        "roles": roles.into_iter().map(|row| -> ApiResult<Value> {
            Ok(json!({
                "key": row.try_get::<String, _>("role_key")?,
                "name": row.try_get::<String, _>("name")?,
                "description": row.try_get::<String, _>("description")?,
                "permissions": row.try_get::<Vec<String>, _>("permissions")?,
            }))
        }).collect::<ApiResult<Vec<_>>>()?
    })))
}

async fn create_app_invitation(
    State(state): State<AppState>,
    user: CurrentUser,
    _csrf: CsrfGuard,
    Path(app_id): Path<String>,
    Json(input): Json<AppInvitationRequest>,
) -> ApiResult<Json<Value>> {
    let app_access =
        access::require_app(&state.pool, &user, &app_id, Capability::MembersManage).await?;
    let email = input.email.trim().to_ascii_lowercase();
    if !email.contains('@') {
        return Err(ApiError::invalid("email must be valid"));
    }
    if email == user.user.email.to_ascii_lowercase() {
        return Err(ApiError::invalid("you already have access to this app"));
    }
    let already_has_access: bool = sqlx::query_scalar(
        r#"
        select exists(
          select 1
          from users u
          join effective_app_permissions eap on eap.user_id = u.id and eap.app_id = $2
          where u.email = $1 and u.status = 'active' and eap.permission = 'app.read'
        )
        "#,
    )
    .bind(&email)
    .bind(&app_id)
    .fetch_one(&state.pool)
    .await?;
    if already_has_access {
        return Err(ApiError::Conflict(
            "this user already has access to the app".to_string(),
        ));
    }
    let role_id = app_role_id(&state.pool, &app_access.workspace_id, &input.role).await?;
    let token = auth::random_token();
    let invitation_id = new_id("inv");
    let expires_at = OffsetDateTime::now_utc() + Duration::days(7);
    let invitation_row = sqlx::query(
        r#"
        insert into app_invitations (
          id, app_id, email, normalized_email, role_id, token_hash,
          invited_by_user_id, expires_at, created_at
        ) values ($1, $2, $3, $3, $4, $5, $6, $7, now())
        on conflict (app_id, normalized_email) where accepted_at is null and revoked_at is null
        do update set role_id = excluded.role_id,
                      token_hash = excluded.token_hash,
                      invited_by_user_id = excluded.invited_by_user_id,
                      expires_at = excluded.expires_at,
                      created_at = now()
        returning id
        "#,
    )
    .bind(&invitation_id)
    .bind(&app_id)
    .bind(&email)
    .bind(&role_id)
    .bind(sha256_hex(token.as_bytes()))
    .bind(&user.user.id)
    .bind(expires_at)
    .fetch_one(&state.pool)
    .await?;
    let invitation_id: String = invitation_row.try_get("id")?;
    access::audit(
        &state.pool,
        &user,
        Some(&app_access.workspace_id),
        Some(&app_id),
        "app.invitation.created",
        Some("invitation"),
        Some(&invitation_id),
        json!({ "email": email, "role": input.role }),
    )
    .await?;
    Ok(Json(json!({
        "invitation": {
            "id": invitation_id,
            "email": email,
            "role": input.role,
            "expires_at": dt(expires_at),
            "invite_token": token,
            "invite_url": format!("{}/invitations/{}", state.config.base_url.trim_end_matches('/'), token)
        }
    })))
}

async fn update_app_member(
    State(state): State<AppState>,
    user: CurrentUser,
    _csrf: CsrfGuard,
    Path((app_id, member_user_id)): Path<(String, String)>,
    Json(input): Json<AppMemberRoleRequest>,
) -> ApiResult<Json<Value>> {
    let app_access =
        access::require_app(&state.pool, &user, &app_id, Capability::MembersManage).await?;
    ensure_manageable_app_member(&state.pool, &app_id, &member_user_id).await?;
    let role_id = app_role_id(&state.pool, &app_access.workspace_id, &input.role).await?;
    let result = sqlx::query(
        "update app_memberships set role_id = $3, granted_by_user_id = $4, updated_at = now() where app_id = $1 and user_id = $2",
    )
    .bind(&app_id)
    .bind(&member_user_id)
    .bind(role_id)
    .bind(&user.user.id)
    .execute(&state.pool)
    .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("app member not found".to_string()));
    }
    access::audit(
        &state.pool,
        &user,
        Some(&app_access.workspace_id),
        Some(&app_id),
        "app.member.role_changed",
        Some("user"),
        Some(&member_user_id),
        json!({ "role": input.role }),
    )
    .await?;
    Ok(Json(json!({ "updated": true })))
}

async fn remove_app_member(
    State(state): State<AppState>,
    user: CurrentUser,
    _csrf: CsrfGuard,
    Path((app_id, member_user_id)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    let app_access =
        access::require_app(&state.pool, &user, &app_id, Capability::MembersManage).await?;
    ensure_manageable_app_member(&state.pool, &app_id, &member_user_id).await?;
    let result = sqlx::query("delete from app_memberships where app_id = $1 and user_id = $2")
        .bind(&app_id)
        .bind(&member_user_id)
        .execute(&state.pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("app member not found".to_string()));
    }
    access::audit(
        &state.pool,
        &user,
        Some(&app_access.workspace_id),
        Some(&app_id),
        "app.member.removed",
        Some("user"),
        Some(&member_user_id),
        json!({}),
    )
    .await?;
    Ok(Json(json!({ "removed": true })))
}

async fn revoke_app_invitation(
    State(state): State<AppState>,
    user: CurrentUser,
    _csrf: CsrfGuard,
    Path((app_id, invitation_id)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    let app_access =
        access::require_app(&state.pool, &user, &app_id, Capability::MembersManage).await?;
    let result = sqlx::query(
        "update app_invitations set revoked_at = now() where id = $1 and app_id = $2 and accepted_at is null and revoked_at is null",
    )
    .bind(&invitation_id)
    .bind(&app_id)
    .execute(&state.pool)
    .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("invitation not found".to_string()));
    }
    access::audit(
        &state.pool,
        &user,
        Some(&app_access.workspace_id),
        Some(&app_id),
        "app.invitation.revoked",
        Some("invitation"),
        Some(&invitation_id),
        json!({}),
    )
    .await?;
    Ok(Json(json!({ "revoked": true })))
}

async fn app_role_id(pool: &sqlx::PgPool, workspace_id: &str, role: &str) -> ApiResult<String> {
    let normalized = role.trim().to_ascii_lowercase();
    if !matches!(
        normalized.as_str(),
        "viewer" | "analyst" | "editor" | "manager"
    ) {
        return Err(ApiError::invalid("unsupported app role"));
    }
    sqlx::query_scalar(
        "select id from app_roles where role_key = $1 and (workspace_id is null or workspace_id = $2) order by workspace_id nulls first limit 1",
    )
    .bind(normalized)
    .bind(workspace_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| ApiError::invalid("app role is not configured"))
}

async fn ensure_manageable_app_member(
    pool: &sqlx::PgPool,
    app_id: &str,
    member_user_id: &str,
) -> ApiResult<()> {
    let row = sqlx::query(
        r#"
        select a.owner_user_id,
               exists(
                 select 1 from workspace_users wu
                 where wu.workspace_id = a.workspace_id
                   and wu.user_id = $2
                   and wu.status = 'active'
                   and wu.role in ('owner', 'admin')
               ) as workspace_admin
        from apps a where a.id = $1 and a.deleted_at is null
        "#,
    )
    .bind(app_id)
    .bind(member_user_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| ApiError::NotFound("app not found".to_string()))?;
    if row.try_get::<String, _>("owner_user_id")? == member_user_id {
        return Err(ApiError::Conflict(
            "app ownership must be transferred instead".to_string(),
        ));
    }
    if row.try_get::<bool, _>("workspace_admin")? {
        return Err(ApiError::Conflict(
            "workspace administrator access cannot be changed from the app".to_string(),
        ));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct DataSourceRequest {
    source_type: String,
    name: String,
    app_id: Option<String>,
    credentials: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct DataSourceCredentialsRequest {
    credentials: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct AppStoreTestNotificationRequest {
    environment: String,
}

#[derive(Debug, Deserialize)]
struct CatchUpRequest {
    from: Option<String>,
    to: Option<String>,
    limit: Option<usize>,
    cursor: Option<String>,
}

async fn list_data_sources(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(filters): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let rows = sqlx::query(
        r#"
        select ds.*, a.name as app_name, a.apple_bundle_id as app_apple_bundle_id,
               a.google_package_name as app_google_package_name
        from data_sources ds
        left join apps a on a.id = ds.app_id
        where has_app_permission($1, ds.app_id, 'app.read')
          and ($2::text is null or ds.app_id = $2)
        order by ds.created_at desc
        "#,
    )
    .bind(&user.user.id)
    .bind(filters.get("app_id"))
    .fetch_all(&state.pool)
    .await?;
    let sources = rows
        .into_iter()
        .map(|row| data_source_json(&state, row))
        .collect::<ApiResult<Vec<_>>>()?;
    Ok(Json(json!({ "data_sources": sources })))
}

async fn create_data_source(
    State(state): State<AppState>,
    user: CurrentUser,
    _csrf: CsrfGuard,
    Json(input): Json<DataSourceRequest>,
) -> ApiResult<Json<Value>> {
    let source_type = normalize_source_type(&input.source_type)?;
    if input.name.trim().is_empty() {
        return Err(ApiError::invalid("name is required"));
    }
    let app_id = input
        .app_id
        .as_deref()
        .ok_or_else(|| ApiError::invalid("app_id is required"))?;
    let credentials_configured = input.credentials.is_some();
    let required = if credentials_configured {
        Capability::SourceCredentialsWrite
    } else {
        Capability::SourceWrite
    };
    let app_access = access::require_app(&state.pool, &user, app_id, required).await?;
    let id = new_id("src");
    let credentials =
        merge_source_credentials(&state, &source_type, app_id, &id, None, input.credentials)
            .await?;
    let (encrypted_credentials, webhook_secret_hash) =
        prepare_source_credentials(&state, credentials)?;
    sqlx::query(
        r#"
        insert into data_sources (
          id, workspace_id, app_id, source_type, name, status, encrypted_credentials, webhook_secret_hash, created_at, updated_at
        )
        values ($1, $2, $3, $4, $5, 'waiting_for_events', $6, $7, now(), now())
        "#,
    )
    .bind(&id)
    .bind(&app_access.workspace_id)
    .bind(app_id)
    .bind(&source_type)
    .bind(input.name.trim())
    .bind(encrypted_credentials)
    .bind(webhook_secret_hash)
    .execute(&state.pool)
    .await?;
    access::audit(
        &state.pool,
        &user,
        Some(&app_access.workspace_id),
        Some(app_id),
        "source.created",
        Some("data_source"),
        Some(&id),
        json!({ "source_type": source_type, "name": input.name.trim(), "credentials_configured": credentials_configured }),
    )
    .await?;
    Ok(Json(
        json!({ "data_source": get_data_source_json(&state, &app_access.workspace_id, &id).await? }),
    ))
}

async fn get_data_source(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(source_id): Path<String>,
) -> ApiResult<Json<Value>> {
    let (_, source_access) =
        authorized_source_row(&state, &user, &source_id, Capability::AppRead).await?;
    Ok(Json(
        json!({ "data_source": get_data_source_json(&state, &source_access.workspace_id, &source_id).await? }),
    ))
}

async fn update_data_source_credentials(
    State(state): State<AppState>,
    user: CurrentUser,
    _csrf: CsrfGuard,
    Path(source_id): Path<String>,
    Json(input): Json<DataSourceCredentialsRequest>,
) -> ApiResult<Json<Value>> {
    let (source, source_access) = authorized_source_row(
        &state,
        &user,
        &source_id,
        Capability::SourceCredentialsWrite,
    )
    .await?;
    let source_type: String = source.try_get("source_type")?;
    let existing_credentials: Option<String> = source.try_get("encrypted_credentials")?;
    let credentials = merge_source_credentials(
        &state,
        &source_type,
        &source_access.app_id,
        &source_id,
        existing_credentials.as_deref(),
        input.credentials,
    )
    .await?;
    let (encrypted_credentials, webhook_secret_hash) =
        prepare_source_credentials(&state, credentials)?;
    sqlx::query(
        r#"
        update data_sources
        set encrypted_credentials = $2,
            webhook_secret_hash = coalesce($3, webhook_secret_hash),
            last_error = null,
            updated_at = now()
        where id = $1
        "#,
    )
    .bind(&source_id)
    .bind(encrypted_credentials)
    .bind(webhook_secret_hash)
    .execute(&state.pool)
    .await?;
    access::audit(
        &state.pool,
        &user,
        Some(&source_access.workspace_id),
        Some(&source_access.app_id),
        "source.credentials_updated",
        Some("data_source"),
        Some(&source_id),
        json!({}),
    )
    .await?;
    Ok(Json(
        json!({ "data_source": get_data_source_json(&state, &source_access.workspace_id, &source_id).await? }),
    ))
}

async fn test_data_source(
    State(state): State<AppState>,
    user: CurrentUser,
    _csrf: CsrfGuard,
    Path(source_id): Path<String>,
) -> ApiResult<Json<Value>> {
    let (source, source_access) =
        authorized_source_row(&state, &user, &source_id, Capability::SourceWrite).await?;
    let sync_id = new_id("syn");
    let status: String = source.try_get("status")?;
    let last_event_at: Option<OffsetDateTime> = source.try_get("last_event_at")?;
    let outcome = if last_event_at.is_some() || status == "active" {
        ("completed", None::<String>)
    } else {
        (
            "completed",
            Some("No events have arrived yet. Send a source test event to the webhook URL to finish setup.".to_string()),
        )
    };
    sqlx::query(
        r#"
        insert into sync_runs (id, workspace_id, app_id, data_source_id, sync_type, status, started_at, finished_at, error)
        values ($1, $2, $3, $4, 'health_check', $5, now(), now(), $6)
        "#,
    )
    .bind(&sync_id)
    .bind(&source_access.workspace_id)
    .bind(&source_access.app_id)
    .bind(&source_id)
    .bind(outcome.0)
    .bind(outcome.1)
    .execute(&state.pool)
    .await?;
    sqlx::query(
        "update data_sources set last_sync_at = now(), last_error = null, updated_at = now() where id = $1",
    )
    .bind(&source_id)
    .execute(&state.pool)
    .await?;
    Ok(Json(
        json!({ "sync_run": get_sync_run_json(&state, &source_access.workspace_id, &sync_id).await? }),
    ))
}

async fn send_app_store_test_notification(
    State(state): State<AppState>,
    user: CurrentUser,
    _csrf: CsrfGuard,
    Path(source_id): Path<String>,
    Json(input): Json<AppStoreTestNotificationRequest>,
) -> ApiResult<Json<Value>> {
    let (source, source_access) =
        authorized_source_row(&state, &user, &source_id, Capability::SourceWrite).await?;
    let source_type: String = source.try_get("source_type")?;
    if source_type != "app_store" {
        return Err(ApiError::invalid(
            "test notifications are only available for App Store sources",
        ));
    }

    let environment = input.environment.trim().to_ascii_lowercase();
    if !matches!(environment.as_str(), "production" | "sandbox") {
        return Err(ApiError::invalid(
            "environment must be production or sandbox",
        ));
    }

    let encrypted_credentials: Option<String> = source.try_get("encrypted_credentials")?;
    let credentials = optional_source_credentials(&state, encrypted_credentials.as_deref())?
        .ok_or_else(|| {
            ApiError::invalid(
                "Configure an App Store In-App Purchase key before sending a test notification",
            )
        })?;
    for key in ["issuer_id", "key_id", "private_key", "bundle_id"] {
        if !credentials
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(|value| !value.trim().is_empty())
        {
            return Err(ApiError::invalid(format!(
                "Configure an App Store In-App Purchase key before sending a test notification ({key} is missing)"
            )));
        }
    }

    let configured_environment = credentials
        .get("environment")
        .and_then(Value::as_str)
        .unwrap_or("both")
        .trim()
        .to_ascii_lowercase();
    if configured_environment != "both" && configured_environment != environment {
        return Err(ApiError::invalid(format!(
            "this source is configured for {configured_environment}; update its environment before sending a {environment} test"
        )));
    }

    let test_notification_token = request_app_store_test_notification(&credentials, &environment)
        .await
        .map_err(|error| {
            ApiError::invalid(format!(
                "Apple couldn't send the test notification. {error}"
            ))
        })?;
    let requested_at = OffsetDateTime::now_utc();
    access::audit(
        &state.pool,
        &user,
        Some(&source_access.workspace_id),
        Some(&source_access.app_id),
        "source.app_store_test_notification_requested",
        Some("data_source"),
        Some(&source_id),
        json!({ "environment": environment }),
    )
    .await?;

    Ok(Json(json!({
        "test_notification": {
            "test_notification_token": test_notification_token,
            "environment": environment,
            "requested_at": dt(requested_at),
        }
    })))
}

async fn catch_up_data_source(
    State(state): State<AppState>,
    user: CurrentUser,
    _csrf: CsrfGuard,
    Path(source_id): Path<String>,
    Json(input): Json<CatchUpRequest>,
) -> ApiResult<Json<Value>> {
    let (source, source_access) =
        authorized_source_row(&state, &user, &source_id, Capability::SourceWrite).await?;
    let source_type: String = source.try_get("source_type")?;
    let sync_id = new_id("syn");
    sqlx::query(
        r#"
        insert into sync_runs (id, workspace_id, app_id, data_source_id, sync_type, status, cursor, started_at)
        values ($1, $2, $3, $4, 'webhook_catch_up', 'running', $5, now())
        "#,
    )
    .bind(&sync_id)
    .bind(&source_access.workspace_id)
    .bind(&source_access.app_id)
    .bind(&source_id)
    .bind(input.cursor.as_deref())
    .execute(&state.pool)
    .await?;
    access::audit(
        &state.pool,
        &user,
        Some(&source_access.workspace_id),
        Some(&source_access.app_id),
        "source.catch_up_started",
        Some("sync_run"),
        Some(&sync_id),
        json!({ "data_source_id": source_id }),
    )
    .await?;

    let result =
        run_webhook_catch_up(&state, &source, &source_id, &source_type, &input, &sync_id).await;
    match result {
        Ok((records_seen, records_inserted, next_cursor)) => {
            sqlx::query(
                r#"
                update sync_runs
                set status = 'completed', finished_at = now(), records_seen = $2, records_inserted = $3, cursor = $4, error = null
                where id = $1
                "#,
            )
            .bind(&sync_id)
            .bind(records_seen)
            .bind(records_inserted)
            .bind(next_cursor.as_deref())
            .execute(&state.pool)
            .await?;
            sqlx::query(
                "update data_sources set last_sync_at = now(), last_error = null, updated_at = now() where id = $1",
            )
            .bind(&source_id)
            .execute(&state.pool)
            .await?;
        }
        Err(error) => {
            let text = error.to_string();
            sqlx::query(
                r#"
                update sync_runs
                set status = 'failed', finished_at = now(), error = $2
                where id = $1
                "#,
            )
            .bind(&sync_id)
            .bind(&text)
            .execute(&state.pool)
            .await?;
            sqlx::query(
                "update data_sources set status = 'error', last_error = $2, updated_at = now() where id = $1",
            )
            .bind(&source_id)
            .bind(&text)
            .execute(&state.pool)
            .await?;
        }
    }

    Ok(Json(
        json!({ "sync_run": get_sync_run_json(&state, &source_access.workspace_id, &sync_id).await? }),
    ))
}

async fn list_logical_products(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(filters): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let rows = sqlx::query(
        r#"
        select lp.*,
          coalesce(json_agg(json_build_object(
            'id', sp.id,
            'source_type', sp.source_type,
            'external_product_id', sp.external_product_id,
            'display_name', sp.display_name,
            'platform', sp.platform
          )) filter (where sp.id is not null), '[]'::json) as source_products
        from logical_products lp
        left join product_mappings pm on pm.logical_product_id = lp.id and pm.active = true
        left join source_products sp on sp.id = pm.source_product_id
        where has_app_permission($1, lp.app_id, 'app.read')
          and ($2::text is null or lp.app_id = $2)
        group by lp.id
        order by lp.created_at desc
        "#,
    )
    .bind(&user.user.id)
    .bind(filters.get("app_id"))
    .fetch_all(&state.pool)
    .await?;
    let products = rows
        .into_iter()
        .map(logical_product_json)
        .collect::<ApiResult<Vec<_>>>()?;
    Ok(Json(json!({ "logical_products": products })))
}

async fn list_source_products(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(filters): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let mut query = QueryBuilder::<Postgres>::new(
        r#"
        select sp.*, ds.name as data_source_name, lp.id as logical_product_id, lp.display_name as logical_product_name
        from source_products sp
        join data_sources ds on ds.id = sp.data_source_id
        left join product_mappings pm on pm.source_product_id = sp.id and pm.active = true
        left join logical_products lp on lp.id = pm.logical_product_id
        where has_app_permission(
        "#,
    );
    query.push_bind(&user.user.id);
    query.push(", sp.app_id, 'app.read')");
    push_optional_filter(&mut query, "sp.app_id", filters.get("app_id"));
    push_optional_filter(
        &mut query,
        "sp.data_source_id",
        filters.get("data_source_id"),
    );
    push_optional_filter(&mut query, "sp.mapping_state", filters.get("mapping_state"));
    push_optional_filter(&mut query, "sp.product_kind", filters.get("product_kind"));
    query.push(" order by sp.mapping_state asc, sp.last_seen_at desc");
    let rows = query.build().fetch_all(&state.pool).await?;
    let products = rows
        .into_iter()
        .map(source_product_json)
        .collect::<ApiResult<Vec<_>>>()?;
    Ok(Json(json!({ "source_products": products })))
}

#[derive(Debug, Deserialize)]
struct CatalogConfirmationRequest {
    app_id: String,
    logical_products: Vec<LogicalProductDraft>,
    mappings: Vec<ProductMappingDraft>,
    ignored_source_product_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct LogicalProductDraft {
    client_id: String,
    existing_logical_product_id: Option<String>,
    display_name: String,
    product_kind: String,
    billing_period: String,
    reporting_category: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProductMappingDraft {
    source_product_id: String,
    logical_product_client_id: String,
    mapping_method: Option<String>,
}

async fn confirm_catalog(
    State(state): State<AppState>,
    user: CurrentUser,
    _csrf: CsrfGuard,
    Json(input): Json<CatalogConfirmationRequest>,
) -> ApiResult<Json<Value>> {
    let app_access =
        access::require_app(&state.pool, &user, &input.app_id, Capability::CatalogWrite).await?;
    if input.logical_products.is_empty() && input.ignored_source_product_ids.is_empty() {
        return Err(ApiError::invalid("nothing to confirm"));
    }
    let mut tx = state.pool.begin().await?;
    let mut logical_by_client = HashMap::new();
    for draft in &input.logical_products {
        if draft.display_name.trim().is_empty() {
            return Err(ApiError::invalid(
                "logical product display_name is required",
            ));
        }
        let id = if let Some(existing) = draft.existing_logical_product_id.as_deref() {
            sqlx::query(
                r#"
                update logical_products
                set display_name = $3, product_kind = $4, billing_period = $5, reporting_category = $6, updated_at = now()
                where app_id = $1 and id = $2
                "#,
            )
            .bind(&input.app_id)
            .bind(existing)
            .bind(draft.display_name.trim())
            .bind(&draft.product_kind)
            .bind(&draft.billing_period)
            .bind(draft.reporting_category.as_deref())
            .execute(&mut *tx)
            .await?;
            existing.to_string()
        } else {
            let id = new_id("lp");
            sqlx::query(
                r#"
                insert into logical_products (
                  id, workspace_id, app_id, display_name, product_kind, billing_period, reporting_category,
                  active, created_from, created_by_user_id, created_at, updated_at
                )
                values ($1, $2, $3, $4, $5, $6, $7, true, 'catalog_confirmation', $8, now(), now())
                "#,
            )
            .bind(&id)
            .bind(&app_access.workspace_id)
            .bind(&input.app_id)
            .bind(draft.display_name.trim())
            .bind(&draft.product_kind)
            .bind(&draft.billing_period)
            .bind(draft.reporting_category.as_deref())
            .bind(&user.user.id)
            .execute(&mut *tx)
            .await?;
            id
        };
        logical_by_client.insert(draft.client_id.clone(), id);
    }

    for mapping in &input.mappings {
        let logical_product_id = logical_by_client
            .get(&mapping.logical_product_client_id)
            .ok_or_else(|| {
                ApiError::invalid("mapping references an unknown logical product draft")
            })?;
        sqlx::query("select id from source_products where app_id = $1 and id = $2")
            .bind(&input.app_id)
            .bind(&mapping.source_product_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| ApiError::NotFound("source product not found".to_string()))?;
        sqlx::query("update product_mappings set active = false where app_id = $1 and source_product_id = $2")
            .bind(&input.app_id)
            .bind(&mapping.source_product_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            r#"
            insert into product_mappings (
              id, workspace_id, app_id, source_product_id, logical_product_id, mapping_method,
              confidence, created_by_user_id, created_at, confirmed_at, active
            )
            values ($1, $2, $3, $4, $5, $6, 1, $7, now(), now(), true)
            "#,
        )
        .bind(new_id("map"))
        .bind(&app_access.workspace_id)
        .bind(&input.app_id)
        .bind(&mapping.source_product_id)
        .bind(logical_product_id)
        .bind(
            mapping
                .mapping_method
                .as_deref()
                .unwrap_or("user_confirmed_catalog_draft"),
        )
        .bind(&user.user.id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "update source_products set mapping_state = 'mapped' where app_id = $1 and id = $2",
        )
        .bind(&input.app_id)
        .bind(&mapping.source_product_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query("update transactions set logical_product_id = $3 where app_id = $1 and source_product_id = $2")
            .bind(&input.app_id)
            .bind(&mapping.source_product_id)
            .bind(logical_product_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("update subscriptions set logical_product_id = $3 where app_id = $1 and source_product_id = $2")
            .bind(&input.app_id)
            .bind(&mapping.source_product_id)
            .bind(logical_product_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("update normalized_events set logical_product_id = $3 where app_id = $1 and source_product_id = $2")
            .bind(&input.app_id)
            .bind(&mapping.source_product_id)
            .bind(logical_product_id)
            .execute(&mut *tx)
            .await?;
    }

    for ignored in &input.ignored_source_product_ids {
        sqlx::query(
            "update source_products set mapping_state = 'ignored', ignored_at = now(), ignored_by_user_id = $3 where app_id = $1 and id = $2",
        )
        .bind(&input.app_id)
        .bind(ignored)
        .bind(&user.user.id)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    access::audit(
        &state.pool,
        &user,
        Some(&app_access.workspace_id),
        Some(&input.app_id),
        "catalog.confirmed",
        Some("app"),
        Some(&input.app_id),
        json!({
            "logical_products": input.logical_products.len(),
            "mappings": input.mappings.len(),
            "ignored": input.ignored_source_product_ids.len()
        }),
    )
    .await?;
    Ok(Json(json!({ "confirmed": true })))
}

async fn list_raw_events(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(filters): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let mut query = QueryBuilder::<Postgres>::new(
        r#"
        select re.*, ds.name as data_source_name, sp.display_name as source_product_name
        from raw_events re
        join data_sources ds on ds.id = re.data_source_id
        left join source_products sp on sp.id = re.source_product_id
        where has_app_permission(
        "#,
    );
    query.push_bind(&user.user.id);
    query.push(", re.app_id, 'events.sensitive.read')");
    push_event_filters(&mut query, &filters, "re");
    query.push(" order by re.received_at desc limit 200");
    let rows = query.build().fetch_all(&state.pool).await?;
    let events = rows
        .into_iter()
        .map(raw_event_json)
        .collect::<ApiResult<Vec<_>>>()?;
    Ok(Json(json!({ "raw_events": events })))
}

async fn get_raw_event(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(event_id): Path<String>,
) -> ApiResult<Json<Value>> {
    let row = sqlx::query(
        r#"
        select re.*, ds.name as data_source_name, sp.display_name as source_product_name
        from raw_events re
        join data_sources ds on ds.id = re.data_source_id
        left join source_products sp on sp.id = re.source_product_id
        where re.id = $2 and has_app_permission($1, re.app_id, 'events.sensitive.read')
        "#,
    )
    .bind(&user.user.id)
    .bind(&event_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| ApiError::NotFound("raw event not found".to_string()))?;
    Ok(Json(json!({ "raw_event": raw_event_json(row)? })))
}

async fn list_normalized_events(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(filters): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let mut query = QueryBuilder::<Postgres>::new(
        r#"
        select ne.*, sp.display_name as source_product_name, lp.display_name as logical_product_name
        from normalized_events ne
        left join source_products sp on sp.id = ne.source_product_id
        left join logical_products lp on lp.id = ne.logical_product_id
        where has_app_permission(
        "#,
    );
    query.push_bind(&user.user.id);
    query.push(", ne.app_id, 'events.sensitive.read')");
    push_normalized_filters(&mut query, &filters);
    query.push(" order by ne.occurred_at desc limit 200");
    let rows = query.build().fetch_all(&state.pool).await?;
    let events = rows
        .into_iter()
        .map(normalized_event_json)
        .collect::<ApiResult<Vec<_>>>()?;
    Ok(Json(json!({ "normalized_events": events })))
}

async fn get_normalized_event(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(event_id): Path<String>,
) -> ApiResult<Json<Value>> {
    let row = sqlx::query(
        r#"
        select ne.*, sp.display_name as source_product_name, lp.display_name as logical_product_name
        from normalized_events ne
        left join source_products sp on sp.id = ne.source_product_id
        left join logical_products lp on lp.id = ne.logical_product_id
        where ne.id = $2 and has_app_permission($1, ne.app_id, 'events.sensitive.read')
        "#,
    )
    .bind(&user.user.id)
    .bind(&event_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| ApiError::NotFound("normalized event not found".to_string()))?;
    Ok(Json(
        json!({ "normalized_event": normalized_event_json(row)? }),
    ))
}

async fn list_transactions(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(filters): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let mut query = QueryBuilder::<Postgres>::new(
        r#"
        select t.*, a.name as app_name, sp.display_name as source_product_name, lp.display_name as logical_product_name
        from transactions t
        left join apps a on a.id = t.app_id
        left join source_products sp on sp.id = t.source_product_id
        left join logical_products lp on lp.id = t.logical_product_id
        where has_app_permission(
        "#,
    );
    query.push_bind(&user.user.id);
    query.push(", t.app_id, 'ledger.read')");
    push_transaction_filters(&mut query, &filters);
    query.push(" order by t.purchase_time desc limit 300");
    let rows = query.build().fetch_all(&state.pool).await?;
    let transactions = rows
        .into_iter()
        .map(transaction_json)
        .collect::<ApiResult<Vec<_>>>()?;
    Ok(Json(json!({ "transactions": transactions })))
}

async fn get_transaction(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(transaction_id): Path<String>,
) -> ApiResult<Json<Value>> {
    let row = sqlx::query(
        r#"
        select t.*, a.name as app_name, sp.display_name as source_product_name, lp.display_name as logical_product_name
        from transactions t
        left join apps a on a.id = t.app_id
        left join source_products sp on sp.id = t.source_product_id
        left join logical_products lp on lp.id = t.logical_product_id
        where t.id = $2 and has_app_permission($1, t.app_id, 'ledger.read')
        "#,
    )
    .bind(&user.user.id)
    .bind(&transaction_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| ApiError::NotFound("transaction not found".to_string()))?;
    let transaction_key: String = row.try_get("transaction_key")?;
    let transaction_app_id: String = row.try_get("app_id")?;
    let evidence_event_id = row
        .try_get::<Option<String>, _>("created_from_event_id")?
        .or(row.try_get::<Option<String>, _>("latest_event_id")?);
    let event_rows = sqlx::query(
        r#"
        select ne.id, ne.event_type, ne.environment, ne.occurred_at, ne.raw_event_id,
               ne.amount_minor, ne.currency, ne.warnings
        from normalized_events ne
        where ne.app_id = $1
          and ne.transaction_key = $2
          and ne.data_source_id = (
            select data_source_id from normalized_events where id = $3
          )
        order by ne.occurred_at desc
        "#,
    )
    .bind(&transaction_app_id)
    .bind(&transaction_key)
    .bind(evidence_event_id.as_deref())
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(json!({
        "transaction": transaction_json(row)?,
        "events": event_rows.into_iter().map(compact_event_json).collect::<ApiResult<Vec<_>>>()?
    })))
}

async fn list_subscriptions(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(filters): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let mut query = QueryBuilder::<Postgres>::new(
        r#"
        select s.*, a.name as app_name, sp.display_name as source_product_name, lp.display_name as logical_product_name
        from subscriptions s
        left join apps a on a.id = s.app_id
        left join source_products sp on sp.id = s.source_product_id
        left join logical_products lp on lp.id = s.logical_product_id
        where has_app_permission(
        "#,
    );
    query.push_bind(&user.user.id);
    query.push(", s.app_id, 'ledger.read')");
    push_optional_filter(&mut query, "s.status", filters.get("status"));
    push_optional_filter(&mut query, "s.app_id", filters.get("app_id"));
    push_optional_filter(&mut query, "s.platform", filters.get("platform"));
    push_optional_filter(&mut query, "s.environment", filters.get("environment"));
    push_optional_filter(
        &mut query,
        "s.logical_product_id",
        filters.get("logical_product_id"),
    );
    push_optional_filter(
        &mut query,
        "s.source_product_id",
        filters.get("source_product_id"),
    );
    query.push(" order by s.updated_at desc limit 300");
    let rows = query.build().fetch_all(&state.pool).await?;
    let subscriptions = rows
        .into_iter()
        .map(subscription_json)
        .collect::<ApiResult<Vec<_>>>()?;
    Ok(Json(json!({ "subscriptions": subscriptions })))
}

async fn get_subscription(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(subscription_id): Path<String>,
) -> ApiResult<Json<Value>> {
    let row = sqlx::query(
        r#"
        select s.*, a.name as app_name, sp.display_name as source_product_name, lp.display_name as logical_product_name
        from subscriptions s
        left join apps a on a.id = s.app_id
        left join source_products sp on sp.id = s.source_product_id
        left join logical_products lp on lp.id = s.logical_product_id
        where s.id = $2 and has_app_permission($1, s.app_id, 'ledger.read')
        "#,
    )
    .bind(&user.user.id)
    .bind(&subscription_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| ApiError::NotFound("subscription not found".to_string()))?;
    let subscription_key: String = row.try_get("subscription_key")?;
    let subscription_app_id: String = row.try_get("app_id")?;
    let timeline = sqlx::query(
        r#"
        select id, event_type, environment, occurred_at, raw_event_id, amount_minor, currency, warnings
        from normalized_events
        where app_id = $1 and subscription_key = $2
        order by occurred_at asc
        "#,
    )
    .bind(&subscription_app_id)
    .bind(subscription_key)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(json!({
        "subscription": subscription_json(row)?,
        "timeline": timeline.into_iter().map(compact_event_json).collect::<ApiResult<Vec<_>>>()?
    })))
}

async fn metrics_overview(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(filters): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let (from, to) = date_range(&filters)?;
    let currency = filters
        .get("currency")
        .cloned()
        .unwrap_or_else(|| "USD".to_string());
    let app_id = filters.get("app_id").map(String::as_str);
    let platform = filters.get("platform").map(String::as_str);
    let product = filters
        .get("logical_product_id")
        .or_else(|| filters.get("product"))
        .map(String::as_str);
    let country = filters.get("country").map(String::as_str);

    let mut revenue = QueryBuilder::<Postgres>::new(
        "select coalesce(sum(ne.amount_minor) filter (where ne.event_type in ('purchase','one_time_purchase','trial_converted','renewal')),0)::bigint as gross, coalesce(sum(abs(ne.amount_minor)) filter (where ne.event_type in ('refund','partial_refund','revocation')),0)::bigint as refunds, count(*) filter (where ne.event_type = 'renewal') as renewals from metric_events ne where has_app_permission(",
    );
    revenue.push_bind(&user.user.id);
    revenue.push(", ne.app_id, 'app.read')");
    revenue.push(" and ne.occurred_at::date between ");
    revenue.push_bind(from);
    revenue.push("::date and ");
    revenue.push_bind(to);
    revenue.push("::date");
    revenue.push(" and ne.currency = ");
    revenue.push_bind(&currency);
    revenue.push(" and ne.environment = 'production'");
    push_optional_filter(&mut revenue, "ne.app_id", app_id);
    push_optional_filter(&mut revenue, "ne.platform", platform);
    push_optional_filter(&mut revenue, "ne.logical_product_id", product);
    push_optional_filter(&mut revenue, "ne.country", country);
    let revenue_row = revenue.build().fetch_one(&state.pool).await?;
    let gross: i64 = revenue_row.try_get("gross")?;
    let refunds: i64 = revenue_row.try_get("refunds")?;
    let renewals: i64 = revenue_row.try_get("renewals")?;

    let mut subs = QueryBuilder::<Postgres>::new(
        "select count(*) filter (where status in ('active','trialing','cancelled_active','grace_period','billing_retry')) as active, count(*) filter (where started_at::date between ",
    );
    subs.push_bind(from);
    subs.push("::date and ");
    subs.push_bind(to);
    subs.push("::date) as new_subs, count(*) filter (where status in ('expired','refunded') and status_updated_at::date between ");
    subs.push_bind(from);
    subs.push("::date and ");
    subs.push_bind(to);
    subs.push("::date) as churned from subscriptions where has_app_permission(");
    subs.push_bind(&user.user.id);
    subs.push(", app_id, 'app.read') and environment = 'production'");
    push_optional_filter(&mut subs, "app_id", app_id);
    push_optional_filter(&mut subs, "platform", platform);
    push_optional_filter(&mut subs, "logical_product_id", product);
    let subs_row = subs.build().fetch_one(&state.pool).await?;
    let active_subscriptions: i64 = subs_row.try_get("active")?;
    let new_subscriptions: i64 = subs_row.try_get("new_subs")?;
    let churned: i64 = subs_row.try_get("churned")?;
    let warnings = metric_warnings(&state, &user.user.id, app_id).await?;
    let refund_rate = if gross <= 0 {
        0.0
    } else {
        refunds as f64 / gross as f64
    };

    Ok(Json(json!({
        "period": { "from": date_s(from), "to": date_s(to) },
        "currency": currency,
        "metrics": {
            "gross_revenue_minor": metric(gross, "gross_revenue_v1", false, trust_state(&warnings)),
            "net_revenue_minor": metric(gross - refunds, "net_revenue_v1", true, "estimated"),
            "refund_amount_minor": metric(refunds, "refund_amount_v1", false, trust_state(&warnings)),
            "active_subscriptions": metric(active_subscriptions, "active_subscriptions_v1", false, trust_state(&warnings)),
            "new_subscriptions": metric(new_subscriptions, "new_subscriptions_v1", false, trust_state(&warnings)),
            "renewals": metric(renewals, "renewals_v1", false, trust_state(&warnings)),
            "churned_subscriptions": metric(churned, "churned_subscriptions_v1", false, trust_state(&warnings)),
            "refund_rate": metric(refund_rate, "refund_rate_v1", false, trust_state(&warnings))
        },
        "warnings": warnings
    })))
}

async fn metrics_revenue_timeseries(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(filters): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let (from, to) = date_range(&filters)?;
    let currency = filters
        .get("currency")
        .cloned()
        .unwrap_or_else(|| "USD".to_string());
    let mut query = QueryBuilder::<Postgres>::new(
        r#"
        select d.date::date as date,
               coalesce(sum(ne.amount_minor) filter (where ne.event_type in ('purchase','one_time_purchase','trial_converted','renewal')),0)::bigint as gross_revenue_minor,
               coalesce(sum(abs(ne.amount_minor)) filter (where ne.event_type in ('refund','partial_refund','revocation')),0)::bigint as refund_amount_minor,
               (coalesce(sum(ne.amount_minor) filter (where ne.event_type in ('purchase','one_time_purchase','trial_converted','renewal')),0) - coalesce(sum(abs(ne.amount_minor)) filter (where ne.event_type in ('refund','partial_refund','revocation')),0))::bigint as net_revenue_minor,
               count(ne.id) filter (where ne.event_type in ('purchase','one_time_purchase','trial_converted')) as purchase_count,
               count(ne.id) filter (where ne.event_type = 'renewal') as renewal_count
        from generate_series(
        "#,
    );
    query.push_bind(from);
    query.push("::date, ");
    query.push_bind(to);
    query.push("::date, interval '1 day') as d(date) left join metric_events ne on ne.occurred_at::date = d.date::date and has_app_permission(");
    query.push_bind(&user.user.id);
    query.push(", ne.app_id, 'app.read') and ne.environment = 'production' and ne.currency = ");
    query.push_bind(&currency);
    push_normalized_filters(&mut query, &filters);
    query.push(" group by d.date::date order by d.date::date asc");
    let rows = query.build().fetch_all(&state.pool).await?;
    Ok(Json(json!({
        "series": rows.into_iter().map(daily_revenue_json).collect::<ApiResult<Vec<_>>>()?
    })))
}

async fn metrics_subscription_timeseries(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(filters): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let (from, to) = date_range(&filters)?;
    let mut query = QueryBuilder::<Postgres>::new(
        r#"
        select d.date::date as date,
               count(distinct coalesce(ne.subscription_key, ne.transaction_key, ne.raw_event_id)) filter (where ne.event_type = 'purchase' and ne.subscription_key is not null) as new_subscription_count,
               count(ne.id) filter (where ne.event_type = 'renewal') as renewal_count,
               count(distinct coalesce(ne.subscription_key, ne.raw_event_id)) filter (where ne.event_type = 'cancellation') as cancel_count,
               count(distinct coalesce(ne.subscription_key, ne.raw_event_id)) filter (where ne.event_type = 'expiration') as expiration_count,
               count(distinct coalesce(ne.subscription_key, ne.transaction_key, ne.raw_event_id)) filter (where ne.event_type = 'trial_started') as trial_start_count,
               count(distinct coalesce(ne.subscription_key, ne.transaction_key, ne.raw_event_id)) filter (where ne.event_type = 'trial_converted') as trial_conversion_count
        from generate_series(
        "#,
    );
    query.push_bind(from);
    query.push("::date, ");
    query.push_bind(to);
    query.push("::date, interval '1 day') as d(date) left join metric_events ne on ne.occurred_at::date = d.date::date and has_app_permission(");
    query.push_bind(&user.user.id);
    query.push(", ne.app_id, 'app.read') and ne.environment = 'production'");
    push_optional_filter(&mut query, "ne.app_id", filters.get("app_id"));
    push_optional_filter(&mut query, "ne.platform", filters.get("platform"));
    push_optional_filter(
        &mut query,
        "ne.logical_product_id",
        filters
            .get("logical_product_id")
            .or_else(|| filters.get("product")),
    );
    query.push(" group by d.date::date order by d.date::date asc");
    let rows = query.build().fetch_all(&state.pool).await?;
    Ok(Json(json!({
        "series": rows.into_iter().map(daily_subscription_json).collect::<ApiResult<Vec<_>>>()?
    })))
}

async fn metrics_breakdown(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(filters): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let (from, to) = date_range(&filters)?;
    let by = filters.get("by").map(String::as_str).unwrap_or("product");
    let select_expr = match by {
        "app" => "coalesce(a.name, 'Unassigned')",
        "platform" => "coalesce(ne.platform, 'unknown')",
        "country" => "coalesce(ne.country, 'unknown')",
        "source" => "coalesce(ds.name, ds.source_type, 'unknown')",
        _ => "coalesce(lp.display_name, 'Unmapped')",
    };
    let mut query = QueryBuilder::<Postgres>::new(format!(
        r#"
        select {select_expr} as label,
               coalesce(sum(ne.amount_minor) filter (where ne.event_type in ('purchase','one_time_purchase','trial_converted','renewal')),0)::bigint as gross_revenue_minor,
               coalesce(sum(abs(ne.amount_minor)) filter (where ne.event_type in ('refund','partial_refund','revocation')),0)::bigint as refund_amount_minor,
               count(ne.id) filter (where ne.event_type in ('purchase','one_time_purchase','trial_converted','renewal')) as transaction_count
        from metric_events ne
        left join logical_products lp on lp.id = ne.logical_product_id
        left join apps a on a.id = ne.app_id
        left join data_sources ds on ds.id = ne.data_source_id
        where has_app_permission(
        "#
    ));
    query.push_bind(&user.user.id);
    query.push(", ne.app_id, 'app.read') and ne.occurred_at::date between ");
    query.push_bind(from);
    query.push("::date and ");
    query.push_bind(to);
    query.push("::date and ne.environment = 'production'");
    if let Some(currency) = filters.get("currency") {
        push_optional_filter(&mut query, "ne.currency", Some(currency));
    }
    push_normalized_filters(&mut query, &filters);
    query.push(" group by label order by gross_revenue_minor desc limit 40");
    let rows = query.build().fetch_all(&state.pool).await?;
    Ok(Json(json!({
        "by": by,
        "items": rows.into_iter().map(breakdown_json).collect::<ApiResult<Vec<_>>>()?
    })))
}

async fn list_sync_runs(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(filters): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let rows = sqlx::query(
        r#"
        select sr.*, ds.name as data_source_name
        from sync_runs sr
        left join data_sources ds on ds.id = sr.data_source_id
        where sr.app_id is not null
          and has_app_permission($1, sr.app_id, 'app.read')
          and ($2::text is null or sr.app_id = $2)
        order by sr.started_at desc
        limit 100
        "#,
    )
    .bind(&user.user.id)
    .bind(filters.get("app_id"))
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(
        json!({ "sync_runs": rows.into_iter().map(sync_run_json).collect::<ApiResult<Vec<_>>>()? }),
    ))
}

async fn get_sync_run(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(sync_run_id): Path<String>,
) -> ApiResult<Json<Value>> {
    Ok(Json(
        json!({ "sync_run": get_authorized_sync_run_json(&state, &user.user.id, &sync_run_id).await? }),
    ))
}

async fn list_jobs(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(filters): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let rows = sqlx::query(
        "select * from jobs where app_id is not null and has_app_permission($1, app_id, 'jobs.run') and ($2::text is null or app_id = $2) order by case status when 'failed' then 0 when 'dead' then 1 when 'running' then 2 else 3 end, created_at desc limit 200",
    )
    .bind(&user.user.id)
    .bind(filters.get("app_id"))
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(
        json!({ "jobs": rows.into_iter().map(job_json).collect::<ApiResult<Vec<_>>>()? }),
    ))
}

async fn retry_job(
    State(state): State<AppState>,
    user: CurrentUser,
    _csrf: CsrfGuard,
    Path(job_id): Path<String>,
) -> ApiResult<Json<Value>> {
    let app_id: String =
        sqlx::query_scalar("select app_id from jobs where id = $1 and app_id is not null")
            .bind(&job_id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(|| ApiError::NotFound("job not found".to_string()))?;
    let app_access = access::require_app(&state.pool, &user, &app_id, Capability::JobsRun).await?;
    sqlx::query("update jobs set status = 'queued', run_after = now(), locked_at = null, locked_by = null, last_error = null where id = $1 and app_id = $2")
        .bind(&job_id)
        .bind(&app_id)
        .execute(&state.pool)
        .await?;
    if let Err(error) = process_normalization_job(&state.pool, &job_id, "api-retry").await {
        tracing::warn!(?error, job_id, "job retry failed");
    }
    access::audit(
        &state.pool,
        &user,
        Some(&app_access.workspace_id),
        Some(&app_id),
        "job.retried",
        Some("job"),
        Some(&job_id),
        json!({}),
    )
    .await?;
    Ok(Json(json!({ "job": get_job_json(&state, &job_id).await? })))
}

struct StoredWebhookPayload {
    raw_event_id: String,
    inserted: bool,
    processing_error: Option<String>,
}

struct WebhookStoreContext<'a> {
    workspace_id: &'a str,
    app_id: &'a str,
    source_id: &'a str,
    source_type: &'a str,
    signature_verified: bool,
    sync_run_id: Option<&'a str>,
    credentials: Option<&'a Value>,
}

async fn run_webhook_catch_up(
    state: &AppState,
    source: &sqlx::postgres::PgRow,
    source_id: &str,
    source_type: &str,
    input: &CatchUpRequest,
    sync_run_id: &str,
) -> ApiResult<(i64, i64, Option<String>)> {
    let workspace_id: String = source.try_get("workspace_id")?;
    let app_id: String = source.try_get("app_id")?;
    let encrypted_credentials: Option<String> = source.try_get("encrypted_credentials")?;
    let credentials = source_credentials(state, encrypted_credentials.as_deref())?;
    let window = catch_up_window(input)?;
    let batch = fetch_webhook_notifications(source_type, &credentials, &window).await?;
    let crate::catchup::CatchUpBatch {
        payloads,
        next_cursor,
        ack,
    } = batch;
    let records_seen = payloads.len() as i64;
    let mut records_inserted = 0_i64;
    let store_context = WebhookStoreContext {
        workspace_id: &workspace_id,
        app_id: &app_id,
        source_id,
        source_type,
        signature_verified: true,
        sync_run_id: Some(sync_run_id),
        credentials: Some(&credentials),
    };

    for payload in payloads {
        if source_type == "app_store" {
            webhook_verification::verify_app_store_payload(&payload, Some(&credentials))
                .map_err(|error| ApiError::Unauthorized(error.to_string()))?;
        } else if source_type == "google_play" {
            webhook_verification::verify_google_play_package(&payload, Some(&credentials))
                .map_err(|error| ApiError::Unauthorized(error.to_string()))?;
        }
        let stored = store_webhook_payload(state, &payload, &store_context).await?;
        if stored.inserted {
            records_inserted += 1;
        }
    }

    if let Some(ack) = ack {
        acknowledge_batch(ack).await?;
    }

    Ok((records_seen, records_inserted, next_cursor))
}

async fn store_webhook_payload(
    state: &AppState,
    payload: &Value,
    context: &WebhookStoreContext<'_>,
) -> ApiResult<StoredWebhookPayload> {
    let WebhookStoreContext {
        workspace_id,
        app_id,
        source_id,
        source_type,
        signature_verified,
        sync_run_id,
        credentials,
    } = *context;
    let processing_payload =
        purchase_lookup::processing_payload(source_type, payload, credentials).await;
    let extraction_payload = processing_payload.as_ref().unwrap_or(payload);
    let fallback = payload_sha256(payload);
    let extracted = extract_event(source_type, extraction_payload, &fallback);
    let raw_id = new_id("raw");
    let sha = payload_sha256(payload);
    let inserted = sqlx::query(
        r#"
        insert into raw_events (
          id, workspace_id, app_id, data_source_id, source_type, source_event_id, source_event_type, environment,
          source_app_id, occurred_at, received_at, payload, processing_payload, payload_sha256,
          signature_verified, processing_status, sync_run_id
        )
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, now(), $11, $12, $13, $14, 'stored', $15)
        on conflict (data_source_id, source_event_id) do nothing
        returning id
        "#,
    )
    .bind(&raw_id)
    .bind(workspace_id)
    .bind(app_id)
    .bind(source_id)
    .bind(source_type)
    .bind(&extracted.source_event_id)
    .bind(&extracted.source_event_type)
    .bind(&extracted.environment)
    .bind(&extracted.source_app_id)
    .bind(extracted.occurred_at)
    .bind(payload)
    .bind(&processing_payload)
    .bind(sha)
    .bind(signature_verified)
    .bind(sync_run_id)
    .fetch_optional(&state.pool)
    .await?;

    let (raw_event_id, inserted) = if let Some(row) = inserted {
        (row.try_get::<String, _>("id")?, true)
    } else {
        let existing_id: String = sqlx::query_scalar(
            "select id from raw_events where data_source_id = $1 and source_event_id = $2",
        )
        .bind(source_id)
        .bind(&extracted.source_event_id)
        .fetch_one(&state.pool)
        .await?;
        (existing_id, false)
    };

    sqlx::query(
        r#"
        update data_sources
        set status = 'active', last_event_at = now(), last_error = null, updated_at = now()
        where id = $1
        "#,
    )
    .bind(source_id)
    .execute(&state.pool)
    .await?;

    if !inserted {
        return Ok(StoredWebhookPayload {
            raw_event_id,
            inserted,
            processing_error: None,
        });
    }

    let job_id = enqueue_normalization(&state.pool, &raw_event_id).await?;
    let processing_error = match process_normalization_job(&state.pool, &job_id, "api-inline").await
    {
        Ok(()) => None,
        Err(error) => {
            let text = error.to_string();
            sqlx::query("update raw_events set processing_status = 'failed', processing_error = $2 where id = $1")
                .bind(&raw_event_id)
                .bind(&text)
                .execute(&state.pool)
                .await?;
            sqlx::query("update data_sources set status = 'error', last_error = $2, updated_at = now() where id = $1")
                .bind(source_id)
                .bind(&text)
                .execute(&state.pool)
                .await?;
            Some(text)
        }
    };

    Ok(StoredWebhookPayload {
        raw_event_id,
        inserted,
        processing_error,
    })
}

async fn ingest_webhook(
    State(state): State<AppState>,
    Path((source_type_path, source_id)): Path<(String, String)>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> ApiResult<Json<Value>> {
    let source_type = normalize_source_type(&source_type_path)?;
    let source = sqlx::query("select * from data_sources where id = $1 and source_type = $2")
        .bind(&source_id)
        .bind(&source_type)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| ApiError::NotFound("data source not found".to_string()))?;
    let workspace_id: String = source.try_get("workspace_id")?;
    let app_id: String = source.try_get("app_id")?;
    let secret_hash: Option<String> = source.try_get("webhook_secret_hash")?;
    let encrypted_credentials: Option<String> = source.try_get("encrypted_credentials")?;
    let credentials = optional_source_credentials(&state, encrypted_credentials.as_deref())?;
    let signature_verified = match source_type.as_str() {
        "app_store" => {
            webhook_verification::verify_app_store_payload(&payload, credentials.as_ref())
                .map_err(|error| {
                    tracing::warn!(source_id, ?error, "rejected unverified App Store webhook");
                    ApiError::Unauthorized(
                        "App Store signed payload verification failed".to_string(),
                    )
                })?;
            true
        }
        "google_play" => {
            let shared_secret_verified = webhook_verification::verify_shared_secret(
                secret_hash.as_deref(),
                &headers,
                &payload,
            );
            let oidc_verified = if shared_secret_verified {
                false
            } else {
                webhook_verification::verify_google_pubsub_oidc(&headers, credentials.as_ref())
                    .await
                    .map_err(|error| {
                        tracing::warn!(
                            source_id,
                            ?error,
                            "rejected unverified Google Pub/Sub webhook"
                        );
                        ApiError::Unauthorized(
                            "Google Pub/Sub push verification failed".to_string(),
                        )
                    })?
            };
            if !oidc_verified && !shared_secret_verified {
                return Err(ApiError::Unauthorized(
                    "Google Play webhooks require Pub/Sub OIDC or a configured shared secret"
                        .to_string(),
                ));
            }
            webhook_verification::verify_google_play_package(&payload, credentials.as_ref())
                .map_err(|error| {
                    tracing::warn!(source_id, ?error, "rejected Google Play package mismatch");
                    ApiError::Unauthorized(
                        "Google Play notification package verification failed".to_string(),
                    )
                })?;
            true
        }
        _ => {
            let verified = webhook_verification::verify_shared_secret(
                secret_hash.as_deref(),
                &headers,
                &payload,
            );
            if secret_hash.is_some() && !verified {
                return Err(ApiError::Unauthorized(
                    "webhook secret verification failed".to_string(),
                ));
            }
            verified
        }
    };
    let store_context = WebhookStoreContext {
        workspace_id: &workspace_id,
        app_id: &app_id,
        source_id: &source_id,
        source_type: &source_type,
        signature_verified,
        sync_run_id: None,
        credentials: credentials.as_ref(),
    };
    let stored = store_webhook_payload(&state, &payload, &store_context).await?;
    Ok(Json(json!({
        "received": true,
        "raw_event_id": stored.raw_event_id,
        "inserted": stored.inserted,
        "signature_verified": signature_verified,
        "processing_error": stored.processing_error
    })))
}

async fn seed_demo(
    State(state): State<AppState>,
    user: CurrentUser,
    _csrf: CsrfGuard,
) -> ApiResult<Json<Value>> {
    let app_id = if let Some(row) =
        sqlx::query("select id from apps where owner_user_id = $1 and deleted_at is null order by created_at asc limit 1")
            .bind(&user.user.id)
            .fetch_optional(&state.pool)
            .await?
    {
        row.try_get("id")?
    } else {
        let app_id = new_id("app");
        sqlx::query(
            "insert into apps (id, workspace_id, owner_user_id, created_by_user_id, name, apple_bundle_id, google_package_name, default_currency, created_at, updated_at) values ($1, $2, $3, $3, 'Tiny Notes', 'com.example.tinynotes', 'com.example.tinynotes', 'USD', now(), now())",
        )
        .bind(&app_id)
        .bind(&user.workspace.id)
        .bind(&user.user.id)
        .execute(&state.pool)
        .await?;
        app_id
    };
    let source_id = if let Some(row) = sqlx::query("select id from data_sources where app_id = $1 and source_type = 'revenuecat' order by created_at asc limit 1")
        .bind(&app_id)
        .fetch_optional(&state.pool)
        .await?
    {
        row.try_get("id")?
    } else {
        let source_id = new_id("src");
        sqlx::query(
            "insert into data_sources (id, workspace_id, app_id, source_type, name, status, webhook_secret_hash, created_at, updated_at) values ($1, $2, $3, 'revenuecat', 'RevenueCat Demo', 'waiting_for_events', $4, now(), now())",
        )
        .bind(&source_id)
        .bind(&user.workspace.id)
        .bind(&app_id)
        .bind(crypto::hash_secret("demo-secret"))
        .execute(&state.pool)
        .await?;
        source_id
    };

    let demo_events = vec![
        json!({"event": {"id": new_id("demoevt"), "type": "INITIAL_PURCHASE", "app_id": "demo", "store": "APP_STORE", "product_id": "com.example.tinynotes.pro.monthly", "app_user_id": "demo_user_1", "transaction_id": "demo_txn_1", "original_transaction_id": "demo_orig_1", "price_in_purchased_currency": 4.99, "currency": "USD", "country_code": "US", "purchased_at_ms": OffsetDateTime::now_utc().unix_timestamp() * 1000 }}),
        json!({"event": {"id": new_id("demoevt"), "type": "RENEWAL", "app_id": "demo", "store": "PLAY_STORE", "product_id": "pro_monthly", "app_user_id": "demo_user_2", "transaction_id": "demo_txn_2", "original_transaction_id": "demo_orig_2", "price_in_purchased_currency": 4.99, "currency": "USD", "country_code": "GB", "purchased_at_ms": (OffsetDateTime::now_utc().unix_timestamp() - 86_400) * 1000 }}),
        json!({"event": {"id": new_id("demoevt"), "type": "NON_RENEWING_PURCHASE", "app_id": "demo", "store": "APP_STORE", "product_id": "com.example.tinynotes.pro.lifetime", "app_user_id": "demo_user_3", "transaction_id": "demo_txn_3", "price_in_purchased_currency": 39.99, "currency": "USD", "country_code": "CA", "purchased_at_ms": (OffsetDateTime::now_utc().unix_timestamp() - 172_800) * 1000 }}),
        json!({"event": {"id": new_id("demoevt"), "type": "REFUND", "app_id": "demo", "store": "APP_STORE", "product_id": "com.example.tinynotes.pro.monthly", "app_user_id": "demo_user_4", "transaction_id": "demo_txn_4", "price_in_purchased_currency": 4.99, "currency": "USD", "country_code": "US", "purchased_at_ms": (OffsetDateTime::now_utc().unix_timestamp() - 43_200) * 1000 }}),
    ];
    let mut inserted = 0;
    for payload in demo_events {
        let fallback = payload_sha256(&payload);
        let extracted = extract_event("revenuecat", &payload, &fallback);
        let raw_id = new_id("raw");
        let row = sqlx::query(
            r#"
            insert into raw_events (
              id, workspace_id, app_id, data_source_id, source_type, source_event_id, source_event_type, environment, source_app_id,
              occurred_at, received_at, payload, payload_sha256, signature_verified, processing_status
            )
            values ($1, $2, $3, $4, 'revenuecat', $5, $6, $7, $8, $9, now(), $10, $11, true, 'stored')
            on conflict (data_source_id, source_event_id) do nothing
            returning id
            "#,
        )
        .bind(&raw_id)
        .bind(&user.workspace.id)
        .bind(&app_id)
        .bind(&source_id)
        .bind(&extracted.source_event_id)
        .bind(&extracted.source_event_type)
        .bind(&extracted.environment)
        .bind(&extracted.source_app_id)
        .bind(extracted.occurred_at)
        .bind(&payload)
        .bind(payload_sha256(&payload))
        .fetch_optional(&state.pool)
        .await?;
        if row.is_some() {
            process_raw_event(&state.pool, &raw_id).await?;
            inserted += 1;
        }
    }
    sqlx::query("update data_sources set status = 'active', last_event_at = now(), updated_at = now() where id = $1")
        .bind(&source_id)
        .execute(&state.pool)
        .await?;

    auto_confirm_demo_products(&state, &user, &app_id).await?;
    Ok(Json(
        json!({ "seeded": true, "events_inserted": inserted, "app_id": app_id, "data_source_id": source_id }),
    ))
}

async fn export_transactions_csv(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(filters): Query<HashMap<String, String>>,
) -> ApiResult<Response> {
    let requested_access = if let Some(app_id) = filters.get("app_id") {
        Some(access::require_app(&state.pool, &user, app_id, Capability::ExportRun).await?)
    } else {
        None
    };
    let rows = sqlx::query(
        r#"
        select t.purchase_time, t.transaction_key, t.source_type, t.platform, t.environment, coalesce(lp.display_name, sp.display_name, 'Unmapped') as product,
               t.amount_minor, t.currency, t.country, t.status
        from transactions t
        left join source_products sp on sp.id = t.source_product_id
        left join logical_products lp on lp.id = t.logical_product_id
        where has_app_permission($1, t.app_id, 'export.run')
          and ($2::text is null or t.app_id = $2)
        order by t.purchase_time desc
        "#,
    )
    .bind(&user.user.id)
    .bind(filters.get("app_id"))
    .fetch_all(&state.pool)
    .await?;
    let mut csv = String::from(
        "purchase_time,transaction_key,source_type,platform,environment,product,amount_minor,currency,country,status\n",
    );
    for row in rows {
        let line = [
            row.try_get::<OffsetDateTime, _>("purchase_time")?
                .to_string(),
            row.try_get::<String, _>("transaction_key")?,
            row.try_get::<String, _>("source_type")?,
            row.try_get::<Option<String>, _>("platform")?
                .unwrap_or_default(),
            row.try_get::<String, _>("environment")?,
            row.try_get::<String, _>("product")?,
            row.try_get::<i64, _>("amount_minor")?.to_string(),
            row.try_get::<String, _>("currency")?,
            row.try_get::<Option<String>, _>("country")?
                .unwrap_or_default(),
            row.try_get::<String, _>("status")?,
        ]
        .into_iter()
        .map(csv_escape)
        .collect::<Vec<_>>()
        .join(",");
        csv.push_str(&line);
        csv.push('\n');
    }
    access::audit(
        &state.pool,
        &user,
        requested_access.as_ref().map(|access| access.workspace_id.as_str()),
        requested_access.as_ref().map(|access| access.app_id.as_str()),
        "transactions.exported",
        Some("portfolio"),
        None,
        json!({ "row_count": csv.lines().count().saturating_sub(1), "app_id": filters.get("app_id") }),
    )
    .await?;
    Ok((
        StatusCode::OK,
        [
            ("content-type", "text/csv; charset=utf-8"),
            (
                "content-disposition",
                "attachment; filename=\"revtern-transactions.csv\"",
            ),
        ],
        csv,
    )
        .into_response())
}

fn auth_mode_name(mode: &AuthMode) -> &'static str {
    match mode {
        AuthMode::Local => "local",
        AuthMode::ReverseProxy => "reverse_proxy",
        AuthMode::Disabled => "disabled",
    }
}

fn registration_mode_name(mode: &RegistrationMode) -> &'static str {
    match mode {
        RegistrationMode::Closed => "closed",
        RegistrationMode::InviteOnly => "invite_only",
        RegistrationMode::Open => "open",
    }
}

fn normalize_source_type(source_type: &str) -> ApiResult<String> {
    let normalized = source_type.replace('-', "_");
    let allowed = [
        "app_store",
        "google_play",
        "revenuecat",
        "stripe",
        "paddle",
        "csv",
        "custom_api",
        "custom",
    ];
    if !allowed.contains(&normalized.as_str()) {
        return Err(ApiError::invalid("unsupported source_type"));
    }
    Ok(if normalized == "custom" {
        "custom_api".to_string()
    } else {
        normalized
    })
}

async fn get_app_for_user_json(state: &AppState, user_id: &str, app_id: &str) -> ApiResult<Value> {
    let row = sqlx::query(
        r#"
        select a.id, a.workspace_id, a.owner_user_id, a.name, a.platform_bundle_id,
               a.apple_bundle_id, a.google_package_name, a.default_currency,
               a.created_at, a.updated_at,
               case
                 when a.owner_user_id = $1 then 'owner'
                 when wu.role in ('owner', 'admin') then 'workspace_admin'
                 else coalesce(ar.role_key, 'viewer')
               end as access_role,
               array_agg(distinct eap.permission order by eap.permission) as permissions
        from apps a
        join effective_app_permissions eap on eap.app_id = a.id and eap.user_id = $1
        left join workspace_users wu
          on wu.workspace_id = a.workspace_id and wu.user_id = $1 and wu.status = 'active'
        left join app_memberships am on am.app_id = a.id and am.user_id = $1
        left join app_roles ar on ar.id = am.role_id
        where a.id = $2 and a.deleted_at is null
        group by a.id, wu.role, ar.role_key
        "#,
    )
    .bind(user_id)
    .bind(app_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| ApiError::NotFound("app not found".to_string()))?;
    app_with_access_json(row)
}

async fn source_row(
    state: &AppState,
    workspace_id: &str,
    source_id: &str,
) -> ApiResult<sqlx::postgres::PgRow> {
    sqlx::query("select * from data_sources where workspace_id = $1 and id = $2")
        .bind(workspace_id)
        .bind(source_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| ApiError::NotFound("data source not found".to_string()))
}

async fn authorized_source_row(
    state: &AppState,
    user: &CurrentUser,
    source_id: &str,
    capability: Capability,
) -> ApiResult<(sqlx::postgres::PgRow, access::AppAccess)> {
    let app_id: String = sqlx::query_scalar("select app_id from data_sources where id = $1")
        .bind(source_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| ApiError::NotFound("data source not found".to_string()))?;
    let app_access = access::require_app(&state.pool, user, &app_id, capability).await?;
    let row = source_row(state, &app_access.workspace_id, source_id).await?;
    Ok((row, app_access))
}

async fn get_data_source_json(
    state: &AppState,
    workspace_id: &str,
    source_id: &str,
) -> ApiResult<Value> {
    let row = sqlx::query(
        r#"
        select ds.*, a.name as app_name, a.apple_bundle_id as app_apple_bundle_id,
               a.google_package_name as app_google_package_name
        from data_sources ds
        left join apps a on a.id = ds.app_id
        where ds.workspace_id = $1 and ds.id = $2
        "#,
    )
    .bind(workspace_id)
    .bind(source_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| ApiError::NotFound("data source not found".to_string()))?;
    data_source_json(state, row)
}

async fn get_sync_run_json(
    state: &AppState,
    workspace_id: &str,
    sync_run_id: &str,
) -> ApiResult<Value> {
    let row = sqlx::query(
        r#"
        select sr.*, ds.name as data_source_name
        from sync_runs sr
        left join data_sources ds on ds.id = sr.data_source_id
        where sr.workspace_id = $1 and sr.id = $2
        "#,
    )
    .bind(workspace_id)
    .bind(sync_run_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| ApiError::NotFound("sync run not found".to_string()))?;
    sync_run_json(row)
}

async fn get_authorized_sync_run_json(
    state: &AppState,
    user_id: &str,
    sync_run_id: &str,
) -> ApiResult<Value> {
    let row = sqlx::query(
        r#"
        select sr.*, ds.name as data_source_name
        from sync_runs sr
        left join data_sources ds on ds.id = sr.data_source_id
        where sr.id = $2
          and sr.app_id is not null
          and has_app_permission($1, sr.app_id, 'app.read')
        "#,
    )
    .bind(user_id)
    .bind(sync_run_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| ApiError::NotFound("sync run not found".to_string()))?;
    sync_run_json(row)
}

async fn get_job_json(state: &AppState, job_id: &str) -> ApiResult<Value> {
    let row = sqlx::query("select * from jobs where id = $1")
        .bind(job_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| ApiError::NotFound("job not found".to_string()))?;
    job_json(row)
}

async fn auto_confirm_demo_products(
    state: &AppState,
    user: &CurrentUser,
    app_id: &str,
) -> ApiResult<()> {
    let rows = sqlx::query(
        "select * from source_products where app_id = $1 and mapping_state = 'unmapped' order by first_seen_at asc",
    )
    .bind(app_id)
    .fetch_all(&state.pool)
    .await?;
    for row in rows {
        let source_product_id: String = row.try_get("id")?;
        let display_name: String = row
            .try_get::<Option<String>, _>("display_name")?
            .unwrap_or_else(|| "Demo Product".to_string());
        let kind: String = row.try_get("product_kind")?;
        let period: String = row.try_get("billing_period")?;
        let lp_id = new_id("lp");
        sqlx::query(
            r#"
            insert into logical_products (id, workspace_id, app_id, display_name, product_kind, billing_period, reporting_category, active, created_from, created_by_user_id, created_at, updated_at)
            values ($1, $2, $3, $4, $5, $6, 'Demo', true, 'demo_seed', $7, now(), now())
            "#,
        )
        .bind(&lp_id)
        .bind(&user.workspace.id)
        .bind(app_id)
        .bind(clean_product_name(&display_name))
        .bind(kind)
        .bind(period)
        .bind(&user.user.id)
        .execute(&state.pool)
        .await?;
        sqlx::query(
            "insert into product_mappings (id, workspace_id, app_id, source_product_id, logical_product_id, mapping_method, confidence, created_by_user_id, created_at, confirmed_at, active) values ($1, $2, $3, $4, $5, 'demo_seed', 1, $6, now(), now(), true)",
        )
        .bind(new_id("map"))
        .bind(&user.workspace.id)
        .bind(app_id)
        .bind(&source_product_id)
        .bind(&lp_id)
        .bind(&user.user.id)
        .execute(&state.pool)
        .await?;
        sqlx::query(
            "update source_products set mapping_state = 'mapped' where app_id = $1 and id = $2",
        )
        .bind(app_id)
        .bind(&source_product_id)
        .execute(&state.pool)
        .await?;
        sqlx::query("update transactions set logical_product_id = $3 where app_id = $1 and source_product_id = $2")
            .bind(app_id)
            .bind(&source_product_id)
            .bind(&lp_id)
            .execute(&state.pool)
            .await?;
        sqlx::query(
            "update subscriptions set logical_product_id = $3 where app_id = $1 and source_product_id = $2",
        )
        .bind(app_id)
        .bind(&source_product_id)
        .bind(&lp_id)
        .execute(&state.pool)
        .await?;
        sqlx::query("update normalized_events set logical_product_id = $3 where app_id = $1 and source_product_id = $2")
            .bind(app_id)
            .bind(&source_product_id)
            .bind(&lp_id)
            .execute(&state.pool)
            .await?;
    }
    sqlx::query(
        r#"
        update normalized_events ne
        set logical_product_id = pm.logical_product_id
        from product_mappings pm
        where ne.app_id = $1
          and pm.app_id = $1
          and pm.source_product_id = ne.source_product_id
          and pm.active = true
          and ne.logical_product_id is null
        "#,
    )
    .bind(app_id)
    .execute(&state.pool)
    .await?;
    Ok(())
}

async fn metric_warnings(
    state: &AppState,
    user_id: &str,
    app_id: Option<&str>,
) -> ApiResult<Vec<String>> {
    let unmapped: i64 = sqlx::query_scalar("select count(*) from source_products where has_app_permission($1, app_id, 'app.read') and ($2::text is null or app_id = $2) and mapping_state = 'unmapped'")
        .bind(user_id)
        .bind(app_id)
        .fetch_one(&state.pool)
        .await?;
    let failed_jobs: i64 =
        sqlx::query_scalar("select count(*) from jobs where app_id is not null and has_app_permission($1, app_id, 'app.read') and ($2::text is null or app_id = $2) and status in ('failed','dead')")
            .bind(user_id)
            .bind(app_id)
            .fetch_one(&state.pool)
            .await?;
    let source_count: i64 =
        sqlx::query_scalar("select count(*) from data_sources where has_app_permission($1, app_id, 'app.read') and ($2::text is null or app_id = $2)")
            .bind(user_id)
            .bind(app_id)
            .fetch_one(&state.pool)
            .await?;
    let transaction_count: i64 =
        sqlx::query_scalar("select count(*) from transactions where has_app_permission($1, app_id, 'app.read') and ($2::text is null or app_id = $2)")
            .bind(user_id)
            .bind(app_id)
            .fetch_one(&state.pool)
            .await?;
    let non_production_transactions: i64 = sqlx::query_scalar(
        "select count(*) from transactions where has_app_permission($1, app_id, 'app.read') and ($2::text is null or app_id = $2) and environment <> 'production'",
    )
    .bind(user_id)
    .bind(app_id)
    .fetch_one(&state.pool)
    .await?;
    let unknown_transactions: i64 = sqlx::query_scalar(
        "select count(*) from transactions where has_app_permission($1, app_id, 'app.read') and ($2::text is null or app_id = $2) and environment = 'unknown'",
    )
    .bind(user_id)
    .bind(app_id)
    .fetch_one(&state.pool)
    .await?;
    let incomplete_money_events: i64 = sqlx::query_scalar(
        r#"
        select count(*)
        from normalized_events
        where has_app_permission($1, app_id, 'app.read')
          and ($2::text is null or app_id = $2)
          and environment = 'production'
          and event_type in ('purchase','one_time_purchase','trial_converted','renewal','refund','partial_refund','revocation')
          and (amount_minor is null or currency is null or currency = 'UNKNOWN')
        "#,
    )
    .bind(user_id)
    .bind(app_id)
    .fetch_one(&state.pool)
    .await?;
    let duplicate_groups: i64 = sqlx::query_scalar(
        r#"
        select count(*)
        from (
          select ne.app_id, ne.event_type, ne.transaction_key
          from normalized_events ne
          where has_app_permission($1, ne.app_id, 'app.read')
            and ($2::text is null or ne.app_id = $2)
            and ne.transaction_key is not null
          group by ne.app_id, ne.event_type, ne.transaction_key
          having count(distinct ne.data_source_id) > 1
        ) duplicates
        "#,
    )
    .bind(user_id)
    .bind(app_id)
    .fetch_one(&state.pool)
    .await?;
    let mut warnings = vec![];
    if source_count == 0 {
        warnings.push("No data source is connected yet.".to_string());
    }
    if transaction_count == 0 {
        warnings.push("No purchase transactions have been projected yet.".to_string());
    }
    if unmapped > 0 {
        warnings.push(format!(
            "{unmapped} source product(s) are unmapped, so product totals may be incomplete."
        ));
    }
    if failed_jobs > 0 {
        warnings.push(format!("{failed_jobs} background job(s) need attention."));
    }
    if non_production_transactions > 0 {
        warnings.push(format!(
            "{non_production_transactions} non-production or unverified transaction(s) are excluded from revenue metrics."
        ));
    }
    if unknown_transactions > 0 {
        warnings.push(
            "Some Google Play transactions are unverified; configure Android Publisher API access to identify test vs production purchases."
                .to_string(),
        );
    }
    if incomplete_money_events > 0 {
        warnings.push(format!(
            "{incomplete_money_events} financial event(s) are missing amount or currency and cannot produce a complete revenue total."
        ));
    }
    if duplicate_groups > 0 {
        warnings.push(format!(
            "{duplicate_groups} cross-source transaction event group(s) were deduplicated in metrics; all source evidence remains in the event ledger."
        ));
    }
    warnings.push("Net revenue is estimated from webhook payloads and may differ from store payout statements.".to_string());
    Ok(warnings)
}

fn date_range(filters: &HashMap<String, String>) -> ApiResult<(Date, Date)> {
    let date_format = format_description!("[year]-[month]-[day]");
    let today = OffsetDateTime::now_utc().date();
    let from = filters
        .get("from")
        .map(|value| Date::parse(value, &date_format))
        .transpose()
        .map_err(|_| ApiError::invalid("from must be YYYY-MM-DD"))?
        .unwrap_or(today - time::Duration::days(29));
    let to = filters
        .get("to")
        .map(|value| Date::parse(value, &date_format))
        .transpose()
        .map_err(|_| ApiError::invalid("to must be YYYY-MM-DD"))?
        .unwrap_or(today);
    if from > to {
        return Err(ApiError::invalid("from must be before to"));
    }
    Ok((from, to))
}

fn catch_up_window(input: &CatchUpRequest) -> ApiResult<CatchUpWindow> {
    let now = OffsetDateTime::now_utc();
    let to = input
        .to
        .as_deref()
        .map(|value| parse_catch_up_time(value, true))
        .transpose()?
        .unwrap_or(now);
    let from = input
        .from
        .as_deref()
        .map(|value| parse_catch_up_time(value, false))
        .transpose()?
        .unwrap_or(to - time::Duration::days(7));
    if from > to {
        return Err(ApiError::invalid("from must be before to"));
    }
    Ok(CatchUpWindow {
        from,
        to,
        limit: input.limit.unwrap_or(100).clamp(1, 500),
        cursor: input.cursor.clone(),
    })
}

fn parse_catch_up_time(value: &str, end_of_day: bool) -> ApiResult<OffsetDateTime> {
    if let Ok(value) = OffsetDateTime::parse(value, &Rfc3339) {
        return Ok(value);
    }
    let date_format = format_description!("[year]-[month]-[day]");
    if let Ok(date) = Date::parse(value, &date_format) {
        let start = date.with_time(time::Time::MIDNIGHT).assume_utc();
        return if end_of_day {
            Ok(start + time::Duration::days(1) - time::Duration::seconds(1))
        } else {
            Ok(start)
        };
    }
    Err(ApiError::invalid(
        "catch-up time must be RFC3339 or YYYY-MM-DD",
    ))
}

fn source_credentials(state: &AppState, encrypted_credentials: Option<&str>) -> ApiResult<Value> {
    let Some(encrypted_credentials) = encrypted_credentials else {
        return Err(ApiError::invalid(
            "catch-up credentials are required for this source",
        ));
    };
    let bytes = crypto::decrypt_json(&state.config.secret_key, encrypted_credentials)?;
    Ok(serde_json::from_slice::<Value>(&bytes)?)
}

fn optional_source_credentials(
    state: &AppState,
    encrypted_credentials: Option<&str>,
) -> ApiResult<Option<Value>> {
    encrypted_credentials
        .map(|value| source_credentials(state, Some(value)))
        .transpose()
}

async fn merge_source_credentials(
    state: &AppState,
    source_type: &str,
    app_id: &str,
    source_id: &str,
    existing_credentials: Option<&str>,
    incoming_credentials: Option<Value>,
) -> ApiResult<Option<Value>> {
    let mut merged = merge_credential_values(
        optional_source_credentials(state, existing_credentials)?,
        incoming_credentials,
    )?;
    let merged_object = merged
        .as_object_mut()
        .ok_or_else(|| ApiError::invalid("source credentials must be a JSON object"))?;

    if source_type == "app_store" {
        let bundle_id: Option<String> =
            sqlx::query_scalar("select apple_bundle_id from apps where id = $1")
                .bind(app_id)
                .fetch_optional(&state.pool)
                .await?
                .flatten();
        let bundle_id = bundle_id
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                ApiError::invalid(
                    "Add the Apple bundle ID to the selected app before connecting App Store",
                )
            })?;
        merged_object.insert("bundle_id".to_string(), json!(bundle_id));
        merged_object
            .entry("environment".to_string())
            .or_insert_with(|| json!("both"));
    }
    if source_type == "google_play" {
        let package_name: Option<String> =
            sqlx::query_scalar("select google_package_name from apps where id = $1")
                .bind(app_id)
                .fetch_optional(&state.pool)
                .await?
                .flatten();
        let package_name = package_name
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                ApiError::invalid(
                    "Add the Google package name to the selected app before connecting Google Play",
                )
            })?;
        merged_object.insert("package_name".to_string(), json!(package_name));
        merged_object.insert(
            "pubsub_oidc_audience".to_string(),
            json!(format!(
                "{}/webhooks/google-play/{source_id}",
                state.config.base_url.trim_end_matches('/')
            )),
        );
    }

    Ok((!merged_object.is_empty()).then_some(merged))
}

fn merge_credential_values(existing: Option<Value>, incoming: Option<Value>) -> ApiResult<Value> {
    let mut merged = existing.unwrap_or_else(|| json!({}));
    let merged_object = merged
        .as_object_mut()
        .ok_or_else(|| ApiError::invalid("source credentials must be a JSON object"))?;
    if let Some(incoming) = incoming {
        let incoming = incoming
            .as_object()
            .ok_or_else(|| ApiError::invalid("source credentials must be a JSON object"))?;
        for (key, value) in incoming {
            let empty_string = value.as_str().is_some_and(|value| value.trim().is_empty());
            if !value.is_null() && !empty_string {
                merged_object.insert(key.clone(), value.clone());
            }
        }
    }
    Ok(merged)
}

#[cfg(test)]
mod source_credentials_tests {
    use super::*;

    #[test]
    fn credential_updates_keep_existing_secrets_when_fields_are_blank() {
        let merged = merge_credential_values(
            Some(json!({
                "issuer_id": "issuer",
                "key_id": "old-key",
                "private_key": "private"
            })),
            Some(json!({
                "environment": "both",
                "key_id": "",
                "private_key": null
            })),
        )
        .expect("merge credentials");

        assert_eq!(merged["issuer_id"], "issuer");
        assert_eq!(merged["key_id"], "old-key");
        assert_eq!(merged["private_key"], "private");
        assert_eq!(merged["environment"], "both");
    }

    #[test]
    fn reads_google_service_account_email_without_exposing_the_private_key() {
        let object_credentials = json!({
            "service_account_json": {
                "client_email": "revtern@example.iam.gserviceaccount.com",
                "private_key": "secret"
            }
        });
        let string_credentials = json!({
            "service_account_json": "{\"client_email\":\"legacy@example.iam.gserviceaccount.com\",\"private_key\":\"secret\"}"
        });

        assert_eq!(
            google_service_account_email(&object_credentials).as_deref(),
            Some("revtern@example.iam.gserviceaccount.com")
        );
        assert_eq!(
            google_service_account_email(&string_credentials).as_deref(),
            Some("legacy@example.iam.gserviceaccount.com")
        );
    }
}

fn prepare_source_credentials(
    state: &AppState,
    credentials: Option<Value>,
) -> ApiResult<(Option<String>, Option<String>)> {
    let mut credentials = credentials.unwrap_or_else(|| json!({}));
    let webhook_secret_hash = credentials
        .get("webhook_secret")
        .or_else(|| credentials.get("authorization"))
        .or_else(|| credentials.get("shared_secret"))
        .and_then(Value::as_str)
        .filter(|secret| !secret.is_empty())
        .map(crypto::hash_secret);

    if let Some(object) = credentials.as_object_mut() {
        object.remove("webhook_secret");
        object.remove("authorization");
        object.remove("shared_secret");
    }

    let encrypted_credentials = if credentials
        .as_object()
        .is_some_and(|object| !object.is_empty())
    {
        Some(crypto::encrypt_json(
            &state.config.secret_key,
            &serde_json::to_vec(&credentials)?,
        )?)
    } else {
        None
    };

    Ok((encrypted_credentials, webhook_secret_hash))
}

fn metric<T: serde::Serialize>(
    value: T,
    definition: &str,
    estimated: bool,
    trust_state: &str,
) -> Value {
    json!({
        "value": value,
        "definition": definition,
        "estimated": estimated,
        "trust_state": trust_state
    })
}

fn trust_state(warnings: &[String]) -> &'static str {
    if warnings.iter().any(|warning| warning.contains("unmapped")) {
        "unmapped"
    } else if warnings.iter().any(|warning| warning.contains("estimated")) {
        "estimated"
    } else {
        "live"
    }
}

fn dt(value: OffsetDateTime) -> String {
    value.format(&Rfc3339).unwrap_or_else(|_| value.to_string())
}

fn opt_dt(value: Option<OffsetDateTime>) -> Option<String> {
    value.map(dt)
}

fn date_s(value: Date) -> String {
    value.to_string()
}

fn push_optional_filter<'a>(
    query: &mut QueryBuilder<'a, Postgres>,
    column: &str,
    value: Option<impl AsRef<str>>,
) {
    if let Some(value) = value
        && !value.as_ref().is_empty()
        && value.as_ref() != "all"
    {
        query.push(" and ");
        query.push(column);
        query.push(" = ");
        query.push_bind(value.as_ref().to_string());
    }
}

fn push_event_filters<'a>(
    query: &mut QueryBuilder<'a, Postgres>,
    filters: &'a HashMap<String, String>,
    alias: &str,
) {
    push_optional_filter(
        query,
        &format!("{alias}.source_type"),
        filters.get("source_type"),
    );
    push_optional_filter(
        query,
        &format!("{alias}.data_source_id"),
        filters.get("data_source_id"),
    );
    push_optional_filter(
        query,
        &format!("{alias}.processing_status"),
        filters.get("processing_status"),
    );
    push_optional_filter(
        query,
        &format!("{alias}.environment"),
        filters.get("environment"),
    );
    if let Some(value) = filters.get("source_event_type") {
        push_optional_filter(query, &format!("{alias}.source_event_type"), Some(value));
    }
    if let Some(from) = filters.get("from") {
        query.push(" and ");
        query.push(alias);
        query.push(".occurred_at::date >= ");
        query.push_bind(from.clone());
        query.push("::date");
    }
    if let Some(to) = filters.get("to") {
        query.push(" and ");
        query.push(alias);
        query.push(".occurred_at::date <= ");
        query.push_bind(to.clone());
        query.push("::date");
    }
    if let Some(q) = filters.get("q")
        && !q.is_empty()
    {
        query.push(" and (");
        query.push(alias);
        query.push(".source_event_id ilike ");
        query.push_bind(format!("%{q}%"));
        query.push(" or ");
        query.push(alias);
        query.push(".payload::text ilike ");
        query.push_bind(format!("%{q}%"));
        query.push(")");
    }
}

fn push_normalized_filters<'a>(
    query: &mut QueryBuilder<'a, Postgres>,
    filters: &'a HashMap<String, String>,
) {
    push_optional_filter(query, "ne.app_id", filters.get("app_id"));
    push_optional_filter(query, "ne.platform", filters.get("platform"));
    push_optional_filter(
        query,
        "ne.logical_product_id",
        filters.get("logical_product_id"),
    );
    push_optional_filter(
        query,
        "ne.source_product_id",
        filters.get("source_product_id"),
    );
    push_optional_filter(query, "ne.country", filters.get("country"));
    push_optional_filter(query, "ne.event_type", filters.get("event_type"));
    push_optional_filter(query, "ne.environment", filters.get("environment"));
}

fn push_transaction_filters<'a>(
    query: &mut QueryBuilder<'a, Postgres>,
    filters: &'a HashMap<String, String>,
) {
    push_optional_filter(query, "t.app_id", filters.get("app_id"));
    push_optional_filter(query, "t.platform", filters.get("platform"));
    push_optional_filter(
        query,
        "t.logical_product_id",
        filters.get("logical_product_id"),
    );
    push_optional_filter(
        query,
        "t.source_product_id",
        filters.get("source_product_id"),
    );
    push_optional_filter(query, "t.country", filters.get("country"));
    push_optional_filter(query, "t.currency", filters.get("currency"));
    push_optional_filter(query, "t.status", filters.get("status"));
    push_optional_filter(query, "t.environment", filters.get("environment"));
    push_optional_filter(query, "t.customer_id", filters.get("customer_id"));
    if let Some(from) = filters.get("from") {
        query.push(" and t.purchase_time::date >= ");
        query.push_bind(from.clone());
        query.push("::date");
    }
    if let Some(to) = filters.get("to") {
        query.push(" and t.purchase_time::date <= ");
        query.push_bind(to.clone());
        query.push("::date");
    }
}

fn app_with_access_json(row: sqlx::postgres::PgRow) -> ApiResult<Value> {
    Ok(json!({
        "id": row.try_get::<String, _>("id")?,
        "workspace_id": row.try_get::<String, _>("workspace_id")?,
        "owner_user_id": row.try_get::<String, _>("owner_user_id")?,
        "name": row.try_get::<String, _>("name")?,
        "platform_bundle_id": row.try_get::<Option<String>, _>("platform_bundle_id")?,
        "apple_bundle_id": row.try_get::<Option<String>, _>("apple_bundle_id")?,
        "google_package_name": row.try_get::<Option<String>, _>("google_package_name")?,
        "default_currency": row.try_get::<Option<String>, _>("default_currency")?,
        "role": row.try_get::<String, _>("access_role")?,
        "permissions": row.try_get::<Vec<String>, _>("permissions")?,
        "created_at": dt(row.try_get::<OffsetDateTime, _>("created_at")?),
        "updated_at": dt(row.try_get::<OffsetDateTime, _>("updated_at")?),
    }))
}

fn data_source_json(state: &AppState, row: sqlx::postgres::PgRow) -> ApiResult<Value> {
    let id: String = row.try_get("id")?;
    let source_type: String = row.try_get("source_type")?;
    let webhook_url = format!(
        "{}/webhooks/{}/{}",
        state.config.base_url.trim_end_matches('/'),
        source_type.replace('_', "-"),
        id
    );
    let encrypted_credentials: Option<String> = row.try_get("encrypted_credentials")?;
    let has_credentials = encrypted_credentials.is_some();
    let credentials = optional_source_credentials(state, encrypted_credentials.as_deref())?;
    let app_apple_bundle_id = row
        .try_get::<Option<String>, _>("app_apple_bundle_id")
        .ok()
        .flatten();
    let app_google_package_name = row
        .try_get::<Option<String>, _>("app_google_package_name")
        .ok()
        .flatten();
    let credential_keys = credentials
        .as_ref()
        .and_then(Value::as_object)
        .map(|object| object.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    let has_google_api_credentials = credential_keys.iter().any(|key| {
        matches!(
            key.as_str(),
            "service_account_json" | "android_publisher_access_token" | "access_token"
        )
    });
    let catch_up_configured = match source_type.as_str() {
        "app_store" => ["issuer_id", "key_id", "private_key"]
            .iter()
            .all(|key| credential_keys.iter().any(|configured| configured == key)),
        "google_play" => {
            credential_keys
                .iter()
                .any(|key| key == "pubsub_subscription" || key == "subscription")
                && has_google_api_credentials
        }
        _ => false,
    };
    let purchase_verification_configured = source_type == "google_play"
        && credential_keys.iter().any(|key| {
            matches!(
                key.as_str(),
                "service_account_json" | "android_publisher_access_token" | "access_token"
            )
        });
    let configuration = match source_type.as_str() {
        "app_store" => json!({
            "bundle_id": credentials.as_ref().and_then(|value| value.get("bundle_id")).and_then(Value::as_str).or(app_apple_bundle_id.as_deref()),
            "environment": credentials.as_ref().and_then(|value| value.get("environment")).and_then(Value::as_str).unwrap_or("both"),
            "app_apple_id": credentials.as_ref().and_then(|value| value.get("app_apple_id")).and_then(Value::as_str),
        }),
        "google_play" => json!({
            "package_name": credentials.as_ref().and_then(|value| value.get("package_name")).and_then(Value::as_str).or(app_google_package_name.as_deref()),
            "pubsub_oidc_audience": credentials.as_ref().and_then(|value| value.get("pubsub_oidc_audience")).and_then(Value::as_str).unwrap_or(webhook_url.as_str()),
            "pubsub_service_account_email": credentials.as_ref().and_then(|value| value.get("pubsub_service_account_email")).and_then(Value::as_str),
            "pubsub_subscription": credentials.as_ref().and_then(|value| value.get("pubsub_subscription").or_else(|| value.get("subscription"))).and_then(Value::as_str),
            "credential_service_account_email": credentials.as_ref().and_then(google_service_account_email),
        }),
        _ => json!({}),
    };
    let app_store_environment = configuration
        .get("environment")
        .and_then(Value::as_str)
        .unwrap_or("both");
    let app_store_identity_configured = credential_keys.iter().any(|key| key == "bundle_id")
        && (app_store_environment == "sandbox"
            || credential_keys.iter().any(|key| key == "app_apple_id"));
    let has_webhook_secret = row
        .try_get::<Option<String>, _>("webhook_secret_hash")?
        .is_some();
    let verification_mode = match source_type.as_str() {
        "app_store" if app_store_identity_configured => "apple_jws",
        "google_play"
            if credential_keys
                .iter()
                .any(|key| key == "pubsub_oidc_audience")
                && credential_keys
                    .iter()
                    .any(|key| key == "pubsub_service_account_email") =>
        {
            "google_oidc"
        }
        _ if has_webhook_secret => "shared_secret",
        _ => "missing",
    };
    Ok(json!({
        "id": id,
        "workspace_id": row.try_get::<String, _>("workspace_id")?,
        "app_id": row.try_get::<Option<String>, _>("app_id")?,
        "app_name": row.try_get::<Option<String>, _>("app_name").ok().flatten(),
        "source_type": source_type,
        "name": row.try_get::<String, _>("name")?,
        "status": row.try_get::<String, _>("status")?,
        "has_credentials": has_credentials,
        "credential_keys": &credential_keys,
        "catch_up_configured": catch_up_configured,
        "purchase_verification_configured": purchase_verification_configured,
        "configuration": configuration,
        "has_webhook_secret": has_webhook_secret,
        "verification_mode": verification_mode,
        "last_event_at": opt_dt(row.try_get::<Option<OffsetDateTime>, _>("last_event_at")?),
        "last_sync_at": opt_dt(row.try_get::<Option<OffsetDateTime>, _>("last_sync_at")?),
        "last_error": row.try_get::<Option<String>, _>("last_error")?,
        "created_at": dt(row.try_get::<OffsetDateTime, _>("created_at")?),
        "updated_at": dt(row.try_get::<OffsetDateTime, _>("updated_at")?),
        "webhook_url": webhook_url,
        "setup_checklist": setup_checklist(&source_type, row.try_get::<Option<OffsetDateTime>, _>("last_event_at")?.is_some(), verification_mode, catch_up_configured, purchase_verification_configured)
    }))
}

fn google_service_account_email(credentials: &Value) -> Option<String> {
    let service_account = credentials.get("service_account_json")?;
    if let Some(object) = service_account.as_object() {
        return object
            .get("client_email")
            .and_then(Value::as_str)
            .map(str::to_string);
    }
    service_account
        .as_str()
        .and_then(|value| serde_json::from_str::<Value>(value).ok())
        .and_then(|value| {
            value
                .get("client_email")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
}

fn setup_checklist(
    source_type: &str,
    has_event: bool,
    verification_mode: &str,
    catch_up_configured: bool,
    purchase_verification_configured: bool,
) -> Vec<Value> {
    match source_type {
        "revenuecat" => vec![
            json!({"key": "webhook_url", "label": "Paste Revtern webhook URL in RevenueCat", "done": true}),
            json!({"key": "secret", "label": "Configure Authorization/shared secret", "done": verification_mode == "shared_secret"}),
            json!({"key": "first_event", "label": "Receive first RevenueCat event", "done": has_event}),
        ],
        "app_store" => vec![
            json!({"key": "notifications", "label": "Configure App Store Server Notification URL", "done": true}),
            json!({"key": "verification", "label": "Configure app identity for signed notification verification", "done": verification_mode == "apple_jws"}),
            json!({"key": "catch_up", "label": "Configure one-click tests and recovery", "done": catch_up_configured, "optional": true}),
            json!({"key": "signed_payload", "label": "Receive and verify a signedPayload notification", "done": has_event}),
        ],
        "google_play" => vec![
            json!({"key": "pubsub", "label": "Configure Pub/Sub push endpoint", "done": true}),
            json!({"key": "verification", "label": "Configure Pub/Sub OIDC audience and service account", "done": verification_mode == "google_oidc" || verification_mode == "shared_secret"}),
            json!({"key": "purchase_verification", "label": "Verify purchases with Android Publisher API", "done": purchase_verification_configured, "optional": true}),
            json!({"key": "catch_up", "label": "Configure missed-notification recovery", "done": catch_up_configured, "optional": true}),
            json!({"key": "rtdn", "label": "Receive RTDN message", "done": has_event}),
        ],
        _ => vec![
            json!({"key": "verification", "label": "Configure a webhook shared secret", "done": verification_mode == "shared_secret"}),
            json!({"key": "webhook", "label": "Send events to the webhook endpoint", "done": has_event}),
            json!({"key": "catalog", "label": "Confirm discovered product catalog", "done": false}),
        ],
    }
}

fn source_product_json(row: sqlx::postgres::PgRow) -> ApiResult<Value> {
    Ok(json!({
        "id": row.try_get::<String, _>("id")?,
        "workspace_id": row.try_get::<String, _>("workspace_id")?,
        "data_source_id": row.try_get::<String, _>("data_source_id")?,
        "data_source_name": row.try_get::<Option<String>, _>("data_source_name").ok().flatten(),
        "app_id": row.try_get::<Option<String>, _>("app_id")?,
        "source_type": row.try_get::<String, _>("source_type")?,
        "platform": row.try_get::<Option<String>, _>("platform")?,
        "external_product_id": row.try_get::<Option<String>, _>("external_product_id")?,
        "external_base_plan_id": row.try_get::<Option<String>, _>("external_base_plan_id")?,
        "external_offer_id": row.try_get::<Option<String>, _>("external_offer_id")?,
        "display_name": row.try_get::<Option<String>, _>("display_name")?,
        "product_kind": row.try_get::<String, _>("product_kind")?,
        "billing_period": row.try_get::<String, _>("billing_period")?,
        "amount_minor": row.try_get::<Option<i64>, _>("amount_minor")?,
        "currency": row.try_get::<Option<String>, _>("currency")?,
        "mapping_state": row.try_get::<String, _>("mapping_state")?,
        "logical_product_id": row.try_get::<Option<String>, _>("logical_product_id").ok().flatten(),
        "logical_product_name": row.try_get::<Option<String>, _>("logical_product_name").ok().flatten(),
        "first_seen_at": dt(row.try_get::<OffsetDateTime, _>("first_seen_at")?),
        "last_seen_at": dt(row.try_get::<OffsetDateTime, _>("last_seen_at")?),
    }))
}

fn logical_product_json(row: sqlx::postgres::PgRow) -> ApiResult<Value> {
    Ok(json!({
        "id": row.try_get::<String, _>("id")?,
        "workspace_id": row.try_get::<String, _>("workspace_id")?,
        "app_id": row.try_get::<Option<String>, _>("app_id")?,
        "display_name": row.try_get::<String, _>("display_name")?,
        "product_kind": row.try_get::<String, _>("product_kind")?,
        "billing_period": row.try_get::<String, _>("billing_period")?,
        "reporting_category": row.try_get::<Option<String>, _>("reporting_category")?,
        "active": row.try_get::<bool, _>("active")?,
        "created_from": row.try_get::<String, _>("created_from")?,
        "created_at": dt(row.try_get::<OffsetDateTime, _>("created_at")?),
        "source_products": row.try_get::<Value, _>("source_products")?,
    }))
}

fn raw_event_json(row: sqlx::postgres::PgRow) -> ApiResult<Value> {
    Ok(json!({
        "id": row.try_get::<String, _>("id")?,
        "workspace_id": row.try_get::<String, _>("workspace_id")?,
        "app_id": row.try_get::<String, _>("app_id")?,
        "data_source_id": row.try_get::<String, _>("data_source_id")?,
        "data_source_name": row.try_get::<Option<String>, _>("data_source_name").ok().flatten(),
        "source_type": row.try_get::<String, _>("source_type")?,
        "source_event_id": row.try_get::<String, _>("source_event_id")?,
        "source_event_type": row.try_get::<Option<String>, _>("source_event_type")?,
        "environment": row.try_get::<String, _>("environment")?,
        "source_app_id": row.try_get::<Option<String>, _>("source_app_id")?,
        "source_product_id": row.try_get::<Option<String>, _>("source_product_id")?,
        "source_product_name": row.try_get::<Option<String>, _>("source_product_name").ok().flatten(),
        "occurred_at": dt(row.try_get::<OffsetDateTime, _>("occurred_at")?),
        "received_at": dt(row.try_get::<OffsetDateTime, _>("received_at")?),
        "payload": row.try_get::<Value, _>("payload")?,
        "processing_payload": row.try_get::<Option<Value>, _>("processing_payload")?,
        "payload_sha256": row.try_get::<String, _>("payload_sha256")?,
        "signature_verified": row.try_get::<bool, _>("signature_verified")?,
        "processing_status": row.try_get::<String, _>("processing_status")?,
        "processing_error": row.try_get::<Option<String>, _>("processing_error")?,
    }))
}

fn normalized_event_json(row: sqlx::postgres::PgRow) -> ApiResult<Value> {
    Ok(json!({
        "id": row.try_get::<String, _>("id")?,
        "raw_event_id": row.try_get::<String, _>("raw_event_id")?,
        "data_source_id": row.try_get::<String, _>("data_source_id")?,
        "app_id": row.try_get::<Option<String>, _>("app_id")?,
        "source_product_id": row.try_get::<Option<String>, _>("source_product_id")?,
        "source_product_name": row.try_get::<Option<String>, _>("source_product_name").ok().flatten(),
        "logical_product_id": row.try_get::<Option<String>, _>("logical_product_id")?,
        "logical_product_name": row.try_get::<Option<String>, _>("logical_product_name").ok().flatten(),
        "event_type": row.try_get::<String, _>("event_type")?,
        "environment": row.try_get::<String, _>("environment")?,
        "platform": row.try_get::<Option<String>, _>("platform")?,
        "customer_key": row.try_get::<Option<String>, _>("customer_key")?,
        "transaction_key": row.try_get::<Option<String>, _>("transaction_key")?,
        "original_transaction_key": row.try_get::<Option<String>, _>("original_transaction_key")?,
        "subscription_key": row.try_get::<Option<String>, _>("subscription_key")?,
        "amount_minor": row.try_get::<Option<i64>, _>("amount_minor")?,
        "currency": row.try_get::<Option<String>, _>("currency")?,
        "country": row.try_get::<Option<String>, _>("country")?,
        "occurred_at": dt(row.try_get::<OffsetDateTime, _>("occurred_at")?),
        "normalization_version": row.try_get::<String, _>("normalization_version")?,
        "confidence": row.try_get::<f64, _>("confidence")?,
        "warnings": row.try_get::<Value, _>("warnings")?,
    }))
}

fn transaction_json(row: sqlx::postgres::PgRow) -> ApiResult<Value> {
    Ok(json!({
        "id": row.try_get::<String, _>("id")?,
        "app_id": row.try_get::<Option<String>, _>("app_id")?,
        "app_name": row.try_get::<Option<String>, _>("app_name").ok().flatten(),
        "source_product_id": row.try_get::<Option<String>, _>("source_product_id")?,
        "source_product_name": row.try_get::<Option<String>, _>("source_product_name").ok().flatten(),
        "logical_product_id": row.try_get::<Option<String>, _>("logical_product_id")?,
        "logical_product_name": row.try_get::<Option<String>, _>("logical_product_name").ok().flatten(),
        "customer_id": row.try_get::<Option<String>, _>("customer_id")?,
        "platform": row.try_get::<Option<String>, _>("platform")?,
        "transaction_key": row.try_get::<String, _>("transaction_key")?,
        "original_transaction_key": row.try_get::<Option<String>, _>("original_transaction_key")?,
        "source_type": row.try_get::<String, _>("source_type")?,
        "environment": row.try_get::<String, _>("environment")?,
        "purchase_time": dt(row.try_get::<OffsetDateTime, _>("purchase_time")?),
        "amount_minor": row.try_get::<i64, _>("amount_minor")?,
        "currency": row.try_get::<String, _>("currency")?,
        "country": row.try_get::<Option<String>, _>("country")?,
        "status": row.try_get::<String, _>("status")?,
        "source_status": row.try_get::<Option<String>, _>("source_status")?,
        "status_reason": row.try_get::<Option<String>, _>("status_reason")?,
        "status_updated_at": dt(row.try_get::<OffsetDateTime, _>("status_updated_at")?),
        "refunded_at": opt_dt(row.try_get::<Option<OffsetDateTime>, _>("refunded_at")?),
        "refund_amount_minor": row.try_get::<Option<i64>, _>("refund_amount_minor")?,
        "created_from_event_id": row.try_get::<Option<String>, _>("created_from_event_id")?,
        "latest_event_id": row.try_get::<Option<String>, _>("latest_event_id")?,
        "updated_at": dt(row.try_get::<OffsetDateTime, _>("updated_at")?),
    }))
}

fn subscription_json(row: sqlx::postgres::PgRow) -> ApiResult<Value> {
    Ok(json!({
        "id": row.try_get::<String, _>("id")?,
        "app_id": row.try_get::<Option<String>, _>("app_id")?,
        "app_name": row.try_get::<Option<String>, _>("app_name").ok().flatten(),
        "source_product_id": row.try_get::<Option<String>, _>("source_product_id")?,
        "source_product_name": row.try_get::<Option<String>, _>("source_product_name").ok().flatten(),
        "logical_product_id": row.try_get::<Option<String>, _>("logical_product_id")?,
        "logical_product_name": row.try_get::<Option<String>, _>("logical_product_name").ok().flatten(),
        "customer_id": row.try_get::<Option<String>, _>("customer_id")?,
        "platform": row.try_get::<Option<String>, _>("platform")?,
        "subscription_key": row.try_get::<String, _>("subscription_key")?,
        "original_transaction_key": row.try_get::<Option<String>, _>("original_transaction_key")?,
        "environment": row.try_get::<String, _>("environment")?,
        "status": row.try_get::<String, _>("status")?,
        "started_at": dt(row.try_get::<OffsetDateTime, _>("started_at")?),
        "current_period_start": opt_dt(row.try_get::<Option<OffsetDateTime>, _>("current_period_start")?),
        "current_period_end": opt_dt(row.try_get::<Option<OffsetDateTime>, _>("current_period_end")?),
        "cancelled_at": opt_dt(row.try_get::<Option<OffsetDateTime>, _>("cancelled_at")?),
        "expired_at": opt_dt(row.try_get::<Option<OffsetDateTime>, _>("expired_at")?),
        "will_renew": row.try_get::<bool, _>("will_renew")?,
        "in_grace_period": row.try_get::<bool, _>("in_grace_period")?,
        "in_billing_retry": row.try_get::<bool, _>("in_billing_retry")?,
        "latest_transaction_id": row.try_get::<Option<String>, _>("latest_transaction_id")?,
        "status_updated_at": dt(row.try_get::<OffsetDateTime, _>("status_updated_at")?),
        "updated_at": dt(row.try_get::<OffsetDateTime, _>("updated_at")?),
    }))
}

fn compact_event_json(row: sqlx::postgres::PgRow) -> ApiResult<Value> {
    Ok(json!({
        "id": row.try_get::<String, _>("id")?,
        "event_type": row.try_get::<String, _>("event_type")?,
        "environment": row.try_get::<String, _>("environment")?,
        "occurred_at": dt(row.try_get::<OffsetDateTime, _>("occurred_at")?),
        "raw_event_id": row.try_get::<String, _>("raw_event_id")?,
        "amount_minor": row.try_get::<Option<i64>, _>("amount_minor").ok().flatten(),
        "currency": row.try_get::<Option<String>, _>("currency").ok().flatten(),
        "warnings": row.try_get::<Value, _>("warnings")?,
    }))
}

fn daily_revenue_json(row: sqlx::postgres::PgRow) -> ApiResult<Value> {
    Ok(json!({
        "date": date_s(row.try_get::<Date, _>("date")?),
        "gross_revenue_minor": row.try_get::<i64, _>("gross_revenue_minor")?,
        "refund_amount_minor": row.try_get::<i64, _>("refund_amount_minor")?,
        "net_revenue_minor": row.try_get::<i64, _>("net_revenue_minor")?,
        "purchase_count": row.try_get::<i64, _>("purchase_count")?,
        "renewal_count": row.try_get::<i64, _>("renewal_count")?,
    }))
}

fn daily_subscription_json(row: sqlx::postgres::PgRow) -> ApiResult<Value> {
    Ok(json!({
        "date": date_s(row.try_get::<Date, _>("date")?),
        "new_subscription_count": row.try_get::<i64, _>("new_subscription_count")?,
        "renewal_count": row.try_get::<i64, _>("renewal_count")?,
        "cancel_count": row.try_get::<i64, _>("cancel_count")?,
        "expiration_count": row.try_get::<i64, _>("expiration_count")?,
        "trial_start_count": row.try_get::<i64, _>("trial_start_count")?,
        "trial_conversion_count": row.try_get::<i64, _>("trial_conversion_count")?,
    }))
}

fn breakdown_json(row: sqlx::postgres::PgRow) -> ApiResult<Value> {
    Ok(json!({
        "label": row.try_get::<String, _>("label")?,
        "gross_revenue_minor": row.try_get::<i64, _>("gross_revenue_minor")?,
        "refund_amount_minor": row.try_get::<i64, _>("refund_amount_minor")?,
        "transaction_count": row.try_get::<i64, _>("transaction_count")?,
    }))
}

fn sync_run_json(row: sqlx::postgres::PgRow) -> ApiResult<Value> {
    Ok(json!({
        "id": row.try_get::<String, _>("id")?,
        "workspace_id": row.try_get::<String, _>("workspace_id")?,
        "app_id": row.try_get::<Option<String>, _>("app_id")?,
        "data_source_id": row.try_get::<Option<String>, _>("data_source_id")?,
        "data_source_name": row.try_get::<Option<String>, _>("data_source_name").ok().flatten(),
        "sync_type": row.try_get::<String, _>("sync_type")?,
        "status": row.try_get::<String, _>("status")?,
        "cursor": row.try_get::<Option<String>, _>("cursor")?,
        "started_at": dt(row.try_get::<OffsetDateTime, _>("started_at")?),
        "finished_at": opt_dt(row.try_get::<Option<OffsetDateTime>, _>("finished_at")?),
        "records_seen": row.try_get::<i64, _>("records_seen")?,
        "records_inserted": row.try_get::<i64, _>("records_inserted")?,
        "error": row.try_get::<Option<String>, _>("error")?,
    }))
}

fn job_json(row: sqlx::postgres::PgRow) -> ApiResult<Value> {
    Ok(json!({
        "id": row.try_get::<String, _>("id")?,
        "workspace_id": row.try_get::<Option<String>, _>("workspace_id")?,
        "app_id": row.try_get::<Option<String>, _>("app_id")?,
        "queue": row.try_get::<String, _>("queue")?,
        "job_type": row.try_get::<String, _>("job_type")?,
        "payload": row.try_get::<Value, _>("payload")?,
        "status": row.try_get::<String, _>("status")?,
        "run_after": dt(row.try_get::<OffsetDateTime, _>("run_after")?),
        "attempts": row.try_get::<i32, _>("attempts")?,
        "max_attempts": row.try_get::<i32, _>("max_attempts")?,
        "locked_at": opt_dt(row.try_get::<Option<OffsetDateTime>, _>("locked_at")?),
        "locked_by": row.try_get::<Option<String>, _>("locked_by")?,
        "last_error": row.try_get::<Option<String>, _>("last_error")?,
        "created_at": dt(row.try_get::<OffsetDateTime, _>("created_at")?),
    }))
}

fn clean_product_name(raw: &str) -> String {
    raw.rsplit('.')
        .next()
        .unwrap_or(raw)
        .replace('_', " ")
        .split_whitespace()
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn csv_escape(value: String) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value
    }
}
