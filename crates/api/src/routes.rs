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
    Date, OffsetDateTime, format_description::well_known::Rfc3339, macros::format_description,
};

use crate::{
    AppState,
    auth::{self, CsrfGuard, CurrentUser},
    catchup::{CatchUpWindow, acknowledge_batch, fetch_webhook_notifications},
    config::AuthMode,
    crypto,
    error::{ApiError, ApiResult},
};

pub fn router(state: AppState) -> Router {
    Router::new()
        .nest(
            "/api",
            Router::new()
                .route("/setup/status", get(setup_status))
                .route("/setup/owner", post(setup_owner))
                .route("/session", post(create_session).delete(delete_session))
                .route("/me", get(me))
                .route("/apps", get(list_apps).post(create_app))
                .route("/apps/{app_id}", patch(update_app))
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
        .route("/webhooks/{source_type}/{source_id}", post(ingest_webhook))
        .with_state(state)
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
        "needs_setup": count == 0 && state.config.auth_mode == AuthMode::SingleUser,
        "auth_mode": auth_mode_name(&state.config.auth_mode),
    })))
}

async fn setup_owner(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(input): Json<SetupOwnerRequest>,
) -> ApiResult<(CookieJar, Json<Value>)> {
    let existing: i64 = sqlx::query_scalar("select count(*) from users")
        .fetch_one(&state.pool)
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
        .execute(&state.pool)
        .await?;
    sqlx::query(
        "insert into users (id, email, password_hash, display_name, role, created_at, last_login_at) values ($1, $2, $3, $4, 'owner', now(), now())",
    )
    .bind(&user_id)
    .bind(input.email.trim().to_ascii_lowercase())
    .bind(password_hash)
    .bind(input.email.trim())
    .execute(&state.pool)
    .await?;
    sqlx::query(
        "insert into workspace_users (workspace_id, user_id, role) values ($1, $2, 'owner')",
    )
    .bind(&workspace_id)
    .bind(&user_id)
    .execute(&state.pool)
    .await?;
    let jar = auth::create_session(&state.pool, &state.config, &user_id, jar).await?;
    Ok((jar, Json(json!({ "created": true }))))
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
    let row = sqlx::query("select id, password_hash from users where email = $1")
        .bind(input.email.trim().to_ascii_lowercase())
        .fetch_optional(&state.pool)
        .await?;
    let row = row.ok_or_else(|| ApiError::Unauthorized("invalid email or password".to_string()))?;
    let user_id: String = row.try_get("id")?;
    let password_hash: String = row.try_get("password_hash")?;
    if !auth::verify_password(&input.password, &password_hash) {
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
        select id, name, platform_bundle_id, apple_bundle_id, google_package_name, default_currency, created_at, updated_at
        from apps
        where workspace_id = $1
        order by created_at asc
        "#,
    )
    .bind(&user.workspace.id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(
        json!({ "apps": rows.into_iter().map(app_json).collect::<ApiResult<Vec<_>>>()? }),
    ))
}

async fn create_app(
    State(state): State<AppState>,
    user: CurrentUser,
    _csrf: CsrfGuard,
    Json(input): Json<AppRequest>,
) -> ApiResult<Json<Value>> {
    if input.name.trim().is_empty() {
        return Err(ApiError::invalid("name is required"));
    }
    let id = new_id("app");
    sqlx::query(
        r#"
        insert into apps (id, workspace_id, name, platform_bundle_id, apple_bundle_id, google_package_name, default_currency, created_at, updated_at)
        values ($1, $2, $3, $4, $5, $6, $7, now(), now())
        "#,
    )
    .bind(&id)
    .bind(&user.workspace.id)
    .bind(input.name.trim())
    .bind(input.platform_bundle_id.as_deref())
    .bind(input.apple_bundle_id.as_deref())
    .bind(input.google_package_name.as_deref())
    .bind(input.default_currency.as_deref())
    .execute(&state.pool)
    .await?;
    Ok(Json(
        json!({ "app": get_app_json(&state, &user.workspace.id, &id).await? }),
    ))
}

async fn update_app(
    State(state): State<AppState>,
    user: CurrentUser,
    _csrf: CsrfGuard,
    Path(app_id): Path<String>,
    Json(input): Json<AppRequest>,
) -> ApiResult<Json<Value>> {
    sqlx::query(
        r#"
        update apps
        set name = $3,
            platform_bundle_id = $4,
            apple_bundle_id = $5,
            google_package_name = $6,
            default_currency = $7,
            updated_at = now()
        where workspace_id = $1 and id = $2
        "#,
    )
    .bind(&user.workspace.id)
    .bind(&app_id)
    .bind(input.name.trim())
    .bind(input.platform_bundle_id.as_deref())
    .bind(input.apple_bundle_id.as_deref())
    .bind(input.google_package_name.as_deref())
    .bind(input.default_currency.as_deref())
    .execute(&state.pool)
    .await?;
    Ok(Json(
        json!({ "app": get_app_json(&state, &user.workspace.id, &app_id).await? }),
    ))
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
struct CatchUpRequest {
    from: Option<String>,
    to: Option<String>,
    limit: Option<usize>,
    cursor: Option<String>,
}

async fn list_data_sources(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Value>> {
    let rows = sqlx::query(
        r#"
        select ds.*, a.name as app_name
        from data_sources ds
        left join apps a on a.id = ds.app_id
        where ds.workspace_id = $1
        order by ds.created_at desc
        "#,
    )
    .bind(&user.workspace.id)
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
    if let Some(app_id) = input.app_id.as_deref() {
        ensure_app(&state, &user.workspace.id, app_id).await?;
    }
    let (encrypted_credentials, webhook_secret_hash) =
        prepare_source_credentials(&state, input.credentials)?;
    let id = new_id("src");
    sqlx::query(
        r#"
        insert into data_sources (
          id, workspace_id, app_id, source_type, name, status, encrypted_credentials, webhook_secret_hash, created_at, updated_at
        )
        values ($1, $2, $3, $4, $5, 'waiting_for_events', $6, $7, now(), now())
        "#,
    )
    .bind(&id)
    .bind(&user.workspace.id)
    .bind(input.app_id.as_deref())
    .bind(&source_type)
    .bind(input.name.trim())
    .bind(encrypted_credentials)
    .bind(webhook_secret_hash)
    .execute(&state.pool)
    .await?;
    Ok(Json(
        json!({ "data_source": get_data_source_json(&state, &user.workspace.id, &id).await? }),
    ))
}

async fn get_data_source(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(source_id): Path<String>,
) -> ApiResult<Json<Value>> {
    Ok(Json(
        json!({ "data_source": get_data_source_json(&state, &user.workspace.id, &source_id).await? }),
    ))
}

async fn update_data_source_credentials(
    State(state): State<AppState>,
    user: CurrentUser,
    _csrf: CsrfGuard,
    Path(source_id): Path<String>,
    Json(input): Json<DataSourceCredentialsRequest>,
) -> ApiResult<Json<Value>> {
    source_row(&state, &user.workspace.id, &source_id).await?;
    let (encrypted_credentials, webhook_secret_hash) =
        prepare_source_credentials(&state, input.credentials)?;
    sqlx::query(
        r#"
        update data_sources
        set encrypted_credentials = $3,
            webhook_secret_hash = coalesce($4, webhook_secret_hash),
            last_error = null,
            updated_at = now()
        where workspace_id = $1 and id = $2
        "#,
    )
    .bind(&user.workspace.id)
    .bind(&source_id)
    .bind(encrypted_credentials)
    .bind(webhook_secret_hash)
    .execute(&state.pool)
    .await?;
    Ok(Json(
        json!({ "data_source": get_data_source_json(&state, &user.workspace.id, &source_id).await? }),
    ))
}

async fn test_data_source(
    State(state): State<AppState>,
    user: CurrentUser,
    _csrf: CsrfGuard,
    Path(source_id): Path<String>,
) -> ApiResult<Json<Value>> {
    let source = source_row(&state, &user.workspace.id, &source_id).await?;
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
        insert into sync_runs (id, workspace_id, data_source_id, sync_type, status, started_at, finished_at, error)
        values ($1, $2, $3, 'health_check', $4, now(), now(), $5)
        "#,
    )
    .bind(&sync_id)
    .bind(&user.workspace.id)
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
        json!({ "sync_run": get_sync_run_json(&state, &user.workspace.id, &sync_id).await? }),
    ))
}

async fn catch_up_data_source(
    State(state): State<AppState>,
    user: CurrentUser,
    _csrf: CsrfGuard,
    Path(source_id): Path<String>,
    Json(input): Json<CatchUpRequest>,
) -> ApiResult<Json<Value>> {
    let source = source_row(&state, &user.workspace.id, &source_id).await?;
    let source_type: String = source.try_get("source_type")?;
    let sync_id = new_id("syn");
    sqlx::query(
        r#"
        insert into sync_runs (id, workspace_id, data_source_id, sync_type, status, cursor, started_at)
        values ($1, $2, $3, 'webhook_catch_up', 'running', $4, now())
        "#,
    )
    .bind(&sync_id)
    .bind(&user.workspace.id)
    .bind(&source_id)
    .bind(input.cursor.as_deref())
    .execute(&state.pool)
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
        json!({ "sync_run": get_sync_run_json(&state, &user.workspace.id, &sync_id).await? }),
    ))
}

async fn list_logical_products(
    State(state): State<AppState>,
    user: CurrentUser,
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
        where lp.workspace_id = $1
        group by lp.id
        order by lp.created_at desc
        "#,
    )
    .bind(&user.workspace.id)
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
        where sp.workspace_id =
        "#,
    );
    query.push_bind(&user.workspace.id);
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
    ensure_app(&state, &user.workspace.id, &input.app_id).await?;
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
                where workspace_id = $1 and id = $2
                "#,
            )
            .bind(&user.workspace.id)
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
            .bind(&user.workspace.id)
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
        sqlx::query("select id from source_products where workspace_id = $1 and id = $2")
            .bind(&user.workspace.id)
            .bind(&mapping.source_product_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| ApiError::NotFound("source product not found".to_string()))?;
        sqlx::query("update product_mappings set active = false where workspace_id = $1 and source_product_id = $2")
            .bind(&user.workspace.id)
            .bind(&mapping.source_product_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            r#"
            insert into product_mappings (
              id, workspace_id, source_product_id, logical_product_id, mapping_method,
              confidence, created_by_user_id, created_at, confirmed_at, active
            )
            values ($1, $2, $3, $4, $5, 1, $6, now(), now(), true)
            "#,
        )
        .bind(new_id("map"))
        .bind(&user.workspace.id)
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
        sqlx::query("update source_products set mapping_state = 'mapped' where workspace_id = $1 and id = $2")
            .bind(&user.workspace.id)
            .bind(&mapping.source_product_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("update transactions set logical_product_id = $3 where workspace_id = $1 and source_product_id = $2")
            .bind(&user.workspace.id)
            .bind(&mapping.source_product_id)
            .bind(logical_product_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("update subscriptions set logical_product_id = $3 where workspace_id = $1 and source_product_id = $2")
            .bind(&user.workspace.id)
            .bind(&mapping.source_product_id)
            .bind(logical_product_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("update normalized_events set logical_product_id = $3 where workspace_id = $1 and source_product_id = $2")
            .bind(&user.workspace.id)
            .bind(&mapping.source_product_id)
            .bind(logical_product_id)
            .execute(&mut *tx)
            .await?;
    }

    for ignored in &input.ignored_source_product_ids {
        sqlx::query(
            "update source_products set mapping_state = 'ignored', ignored_at = now(), ignored_by_user_id = $3 where workspace_id = $1 and id = $2",
        )
        .bind(&user.workspace.id)
        .bind(ignored)
        .bind(&user.user.id)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
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
        where re.workspace_id =
        "#,
    );
    query.push_bind(&user.workspace.id);
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
        where re.workspace_id = $1 and re.id = $2
        "#,
    )
    .bind(&user.workspace.id)
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
        where ne.workspace_id =
        "#,
    );
    query.push_bind(&user.workspace.id);
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
        where ne.workspace_id = $1 and ne.id = $2
        "#,
    )
    .bind(&user.workspace.id)
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
        where t.workspace_id =
        "#,
    );
    query.push_bind(&user.workspace.id);
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
        where t.workspace_id = $1 and t.id = $2
        "#,
    )
    .bind(&user.workspace.id)
    .bind(&transaction_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| ApiError::NotFound("transaction not found".to_string()))?;
    let event_rows = sqlx::query(
        r#"
        select ne.id, ne.event_type, ne.occurred_at, ne.raw_event_id, ne.warnings
        from normalized_events ne
        join transactions t on t.created_from_event_id = ne.id or t.latest_event_id = ne.id
        where t.id = $1
        order by ne.occurred_at desc
        "#,
    )
    .bind(&transaction_id)
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
        where s.workspace_id =
        "#,
    );
    query.push_bind(&user.workspace.id);
    push_optional_filter(&mut query, "s.status", filters.get("status"));
    push_optional_filter(&mut query, "s.app_id", filters.get("app_id"));
    push_optional_filter(&mut query, "s.platform", filters.get("platform"));
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
    push_optional_filter(&mut query, "s.country", filters.get("country"));
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
        where s.workspace_id = $1 and s.id = $2
        "#,
    )
    .bind(&user.workspace.id)
    .bind(&subscription_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| ApiError::NotFound("subscription not found".to_string()))?;
    let subscription_key: String = row.try_get("subscription_key")?;
    let timeline = sqlx::query(
        r#"
        select id, event_type, occurred_at, raw_event_id, amount_minor, currency, warnings
        from normalized_events
        where workspace_id = $1 and subscription_key = $2
        order by occurred_at asc
        "#,
    )
    .bind(&user.workspace.id)
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
        "select coalesce(sum(amount_minor) filter (where status in ('paid','renewed')),0)::bigint as gross, coalesce(sum(refund_amount_minor),0)::bigint as refunds, count(*) filter (where status in ('paid','renewed')) as purchases, count(*) filter (where status = 'renewed') as renewals from transactions where workspace_id = ",
    );
    revenue.push_bind(&user.workspace.id);
    revenue.push(" and purchase_time::date between ");
    revenue.push_bind(from);
    revenue.push(" and ");
    revenue.push_bind(to);
    revenue.push(" and currency = ");
    revenue.push_bind(&currency);
    push_optional_filter(&mut revenue, "app_id", app_id);
    push_optional_filter(&mut revenue, "platform", platform);
    push_optional_filter(&mut revenue, "logical_product_id", product);
    push_optional_filter(&mut revenue, "country", country);
    let revenue_row = revenue.build().fetch_one(&state.pool).await?;
    let gross: i64 = revenue_row.try_get("gross")?;
    let refunds: i64 = revenue_row.try_get("refunds")?;
    let purchases: i64 = revenue_row.try_get("purchases")?;
    let renewals: i64 = revenue_row.try_get("renewals")?;

    let mut subs = QueryBuilder::<Postgres>::new(
        "select count(*) filter (where status in ('active','trialing','cancelled_active','grace_period','billing_retry')) as active, count(*) filter (where started_at::date between ",
    );
    subs.push_bind(from);
    subs.push(" and ");
    subs.push_bind(to);
    subs.push(") as new_subs, count(*) filter (where status in ('expired','refunded')) as churned from subscriptions where workspace_id = ");
    subs.push_bind(&user.workspace.id);
    push_optional_filter(&mut subs, "app_id", app_id);
    push_optional_filter(&mut subs, "platform", platform);
    push_optional_filter(&mut subs, "logical_product_id", product);
    let subs_row = subs.build().fetch_one(&state.pool).await?;
    let active_subscriptions: i64 = subs_row.try_get("active")?;
    let new_subscriptions: i64 = subs_row.try_get("new_subs")?;
    let churned: i64 = subs_row.try_get("churned")?;
    let warnings = metric_warnings(&state, &user.workspace.id).await?;
    let refund_rate = if purchases == 0 {
        0.0
    } else {
        refunds as f64 / gross.max(1) as f64
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
    let rows = sqlx::query(
        r#"
        select purchase_time::date as date,
               coalesce(sum(amount_minor) filter (where status in ('paid','renewed')),0)::bigint as gross_revenue_minor,
               coalesce(sum(refund_amount_minor),0)::bigint as refund_amount_minor,
               (coalesce(sum(amount_minor) filter (where status in ('paid','renewed')),0) - coalesce(sum(refund_amount_minor),0))::bigint as net_revenue_minor,
               count(*) filter (where status in ('paid','renewed')) as purchase_count,
               count(*) filter (where status = 'renewed') as renewal_count
        from transactions
        where workspace_id = $1 and purchase_time::date between $2 and $3 and currency = $4
        group by purchase_time::date
        order by date asc
        "#,
    )
    .bind(&user.workspace.id)
    .bind(from)
    .bind(to)
    .bind(currency)
    .fetch_all(&state.pool)
    .await?;
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
    let rows = sqlx::query(
        r#"
        with days as (
          select generate_series($1::date, $2::date, interval '1 day')::date as date
        ),
        new_subs as (
          select started_at::date as date, count(*) as value
          from subscriptions
          where workspace_id = $3 and started_at::date between $1 and $2
          group by started_at::date
        ),
        renewals as (
          select purchase_time::date as date, count(*) as value
          from transactions
          where workspace_id = $3 and status = 'renewed' and purchase_time::date between $1 and $2
          group by purchase_time::date
        ),
        cancels as (
          select cancelled_at::date as date, count(*) as value
          from subscriptions
          where workspace_id = $3 and cancelled_at is not null and cancelled_at::date between $1 and $2
          group by cancelled_at::date
        ),
        expirations as (
          select expired_at::date as date, count(*) as value
          from subscriptions
          where workspace_id = $3 and expired_at is not null and expired_at::date between $1 and $2
          group by expired_at::date
        ),
        trial_starts as (
          select occurred_at::date as date, count(distinct coalesce(subscription_key, transaction_key, raw_event_id)) as value
          from normalized_events
          where workspace_id = $3 and event_type = 'trial_started' and occurred_at::date between $1 and $2
          group by occurred_at::date
        ),
        trial_conversions as (
          select occurred_at::date as date, count(distinct coalesce(subscription_key, transaction_key, raw_event_id)) as value
          from normalized_events
          where workspace_id = $3 and event_type = 'trial_converted' and occurred_at::date between $1 and $2
          group by occurred_at::date
        ),
        series as (
          select d.date,
                 coalesce(new_subs.value,0)::bigint as new_subscription_count,
                 coalesce(renewals.value,0)::bigint as renewal_count,
                 coalesce(cancels.value,0)::bigint as cancel_count,
                 coalesce(expirations.value,0)::bigint as expiration_count,
                 coalesce(trial_starts.value,0)::bigint as trial_start_count,
                 coalesce(trial_conversions.value,0)::bigint as trial_conversion_count
          from days d
          left join new_subs on new_subs.date = d.date
          left join renewals on renewals.date = d.date
          left join cancels on cancels.date = d.date
          left join expirations on expirations.date = d.date
          left join trial_starts on trial_starts.date = d.date
          left join trial_conversions on trial_conversions.date = d.date
        )
        select *
        from series
        where new_subscription_count + renewal_count + cancel_count + expiration_count + trial_start_count + trial_conversion_count > 0
        order by date asc
        "#,
    )
    .bind(from)
    .bind(to)
    .bind(&user.workspace.id)
    .fetch_all(&state.pool)
    .await?;
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
    let (select_expr, join_expr) = match by {
        "app" => (
            "coalesce(a.name, 'Unassigned')",
            "left join apps a on a.id = t.app_id left join logical_products lp on false",
        ),
        "platform" => (
            "coalesce(t.platform, 'unknown')",
            "left join logical_products lp on false left join apps a on false",
        ),
        "country" => (
            "coalesce(t.country, 'unknown')",
            "left join logical_products lp on false left join apps a on false",
        ),
        "source" => (
            "t.source_type",
            "left join logical_products lp on false left join apps a on false",
        ),
        _ => (
            "coalesce(lp.display_name, 'Unmapped')",
            "left join logical_products lp on lp.id = t.logical_product_id left join apps a on false",
        ),
    };
    let sql = format!(
        r#"
        select {select_expr} as label, coalesce(sum(t.amount_minor) filter (where t.status in ('paid','renewed')),0)::bigint as gross_revenue_minor,
               coalesce(sum(t.refund_amount_minor),0)::bigint as refund_amount_minor,
               count(*) as transaction_count
        from transactions t
        {join_expr}
        where t.workspace_id = $1 and t.purchase_time::date between $2 and $3
        group by label
        order by gross_revenue_minor desc
        limit 40
        "#
    );
    let rows = sqlx::query(&sql)
        .bind(&user.workspace.id)
        .bind(from)
        .bind(to)
        .fetch_all(&state.pool)
        .await?;
    Ok(Json(json!({
        "by": by,
        "items": rows.into_iter().map(breakdown_json).collect::<ApiResult<Vec<_>>>()?
    })))
}

async fn list_sync_runs(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Value>> {
    let rows = sqlx::query(
        r#"
        select sr.*, ds.name as data_source_name
        from sync_runs sr
        left join data_sources ds on ds.id = sr.data_source_id
        where sr.workspace_id = $1
        order by sr.started_at desc
        limit 100
        "#,
    )
    .bind(&user.workspace.id)
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
        json!({ "sync_run": get_sync_run_json(&state, &user.workspace.id, &sync_run_id).await? }),
    ))
}

async fn list_jobs(State(state): State<AppState>, _user: CurrentUser) -> ApiResult<Json<Value>> {
    let rows = sqlx::query(
        "select * from jobs order by case status when 'failed' then 0 when 'dead' then 1 when 'running' then 2 else 3 end, created_at desc limit 200",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(
        json!({ "jobs": rows.into_iter().map(job_json).collect::<ApiResult<Vec<_>>>()? }),
    ))
}

async fn retry_job(
    State(state): State<AppState>,
    _user: CurrentUser,
    _csrf: CsrfGuard,
    Path(job_id): Path<String>,
) -> ApiResult<Json<Value>> {
    sqlx::query("update jobs set status = 'queued', run_after = now(), locked_at = null, locked_by = null, last_error = null where id = $1")
        .bind(&job_id)
        .execute(&state.pool)
        .await?;
    if let Err(error) = process_normalization_job(&state.pool, &job_id, "api-retry").await {
        tracing::warn!(?error, job_id, "job retry failed");
    }
    Ok(Json(json!({ "job": get_job_json(&state, &job_id).await? })))
}

struct StoredWebhookPayload {
    raw_event_id: String,
    inserted: bool,
    processing_error: Option<String>,
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

    for payload in payloads {
        let stored = store_webhook_payload(
            state,
            &workspace_id,
            source_id,
            source_type,
            &payload,
            true,
            Some(sync_run_id),
        )
        .await?;
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
    workspace_id: &str,
    source_id: &str,
    source_type: &str,
    payload: &Value,
    signature_verified: bool,
    sync_run_id: Option<&str>,
) -> ApiResult<StoredWebhookPayload> {
    let processing_payload: Option<Value> = None;
    let extraction_payload = processing_payload.as_ref().unwrap_or(payload);
    let fallback = payload_sha256(payload);
    let extracted = extract_event(source_type, extraction_payload, &fallback);
    let raw_id = new_id("raw");
    let sha = payload_sha256(payload);
    let inserted = sqlx::query(
        r#"
        insert into raw_events (
          id, workspace_id, data_source_id, source_type, source_event_id, source_event_type,
          source_app_id, occurred_at, received_at, payload, processing_payload, payload_sha256,
          signature_verified, processing_status, sync_run_id
        )
        values ($1, $2, $3, $4, $5, $6, $7, $8, now(), $9, $10, $11, $12, 'stored', $13)
        on conflict (data_source_id, source_event_id) do nothing
        returning id
        "#,
    )
    .bind(&raw_id)
    .bind(workspace_id)
    .bind(source_id)
    .bind(source_type)
    .bind(&extracted.source_event_id)
    .bind(&extracted.source_event_type)
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
    let secret_hash: Option<String> = source.try_get("webhook_secret_hash")?;
    let signature_verified = verify_webhook_secret(secret_hash.as_deref(), &headers, &payload);
    let stored = store_webhook_payload(
        &state,
        &workspace_id,
        &source_id,
        &source_type,
        &payload,
        signature_verified,
        None,
    )
    .await?;
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
        sqlx::query("select id from apps where workspace_id = $1 order by created_at asc limit 1")
            .bind(&user.workspace.id)
            .fetch_optional(&state.pool)
            .await?
    {
        row.try_get("id")?
    } else {
        let app_id = new_id("app");
        sqlx::query(
            "insert into apps (id, workspace_id, name, apple_bundle_id, google_package_name, default_currency, created_at, updated_at) values ($1, $2, 'Tiny Notes', 'com.example.tinynotes', 'com.example.tinynotes', 'USD', now(), now())",
        )
        .bind(&app_id)
        .bind(&user.workspace.id)
        .execute(&state.pool)
        .await?;
        app_id
    };
    let source_id = if let Some(row) = sqlx::query("select id from data_sources where workspace_id = $1 and source_type = 'revenuecat' order by created_at asc limit 1")
        .bind(&user.workspace.id)
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
              id, workspace_id, data_source_id, source_type, source_event_id, source_event_type, source_app_id,
              occurred_at, received_at, payload, payload_sha256, signature_verified, processing_status
            )
            values ($1, $2, $3, 'revenuecat', $4, $5, $6, $7, now(), $8, $9, true, 'stored')
            on conflict (data_source_id, source_event_id) do nothing
            returning id
            "#,
        )
        .bind(&raw_id)
        .bind(&user.workspace.id)
        .bind(&source_id)
        .bind(&extracted.source_event_id)
        .bind(&extracted.source_event_type)
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
) -> ApiResult<Response> {
    let rows = sqlx::query(
        r#"
        select t.purchase_time, t.transaction_key, t.source_type, t.platform, coalesce(lp.display_name, sp.display_name, 'Unmapped') as product,
               t.amount_minor, t.currency, t.country, t.status
        from transactions t
        left join source_products sp on sp.id = t.source_product_id
        left join logical_products lp on lp.id = t.logical_product_id
        where t.workspace_id = $1
        order by t.purchase_time desc
        "#,
    )
    .bind(&user.workspace.id)
    .fetch_all(&state.pool)
    .await?;
    let mut csv = String::from(
        "purchase_time,transaction_key,source_type,platform,product,amount_minor,currency,country,status\n",
    );
    for row in rows {
        let line = [
            row.try_get::<OffsetDateTime, _>("purchase_time")?
                .to_string(),
            row.try_get::<String, _>("transaction_key")?,
            row.try_get::<String, _>("source_type")?,
            row.try_get::<Option<String>, _>("platform")?
                .unwrap_or_default(),
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
        AuthMode::SingleUser => "single_user",
        AuthMode::ReverseProxy => "reverse_proxy",
        AuthMode::Disabled => "disabled",
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

fn verify_webhook_secret(secret_hash: Option<&str>, headers: &HeaderMap, payload: &Value) -> bool {
    let Some(secret_hash) = secret_hash else {
        return false;
    };
    let header_secret = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim_start_matches("Bearer ").trim().to_string())
        .or_else(|| {
            headers
                .get("x-revtern-webhook-secret")
                .and_then(|value| value.to_str().ok())
                .map(str::to_string)
        })
        .or_else(|| {
            headers
                .get("x-revenuecat-authorization")
                .and_then(|value| value.to_str().ok())
                .map(str::to_string)
        });
    if let Some(candidate) = header_secret {
        return sha256_hex(candidate.as_bytes()) == secret_hash;
    }
    payload
        .get("shared_secret")
        .and_then(Value::as_str)
        .is_some_and(|candidate| sha256_hex(candidate.as_bytes()) == secret_hash)
}

async fn ensure_app(state: &AppState, workspace_id: &str, app_id: &str) -> ApiResult<()> {
    sqlx::query("select id from apps where workspace_id = $1 and id = $2")
        .bind(workspace_id)
        .bind(app_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| ApiError::NotFound("app not found".to_string()))?;
    Ok(())
}

async fn get_app_json(state: &AppState, workspace_id: &str, app_id: &str) -> ApiResult<Value> {
    let row = sqlx::query("select * from apps where workspace_id = $1 and id = $2")
        .bind(workspace_id)
        .bind(app_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| ApiError::NotFound("app not found".to_string()))?;
    app_json(row)
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

async fn get_data_source_json(
    state: &AppState,
    workspace_id: &str,
    source_id: &str,
) -> ApiResult<Value> {
    let row = sqlx::query(
        r#"
        select ds.*, a.name as app_name
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
        "select * from source_products where workspace_id = $1 and mapping_state = 'unmapped' order by first_seen_at asc",
    )
    .bind(&user.workspace.id)
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
            "insert into product_mappings (id, workspace_id, source_product_id, logical_product_id, mapping_method, confidence, created_by_user_id, created_at, confirmed_at, active) values ($1, $2, $3, $4, 'demo_seed', 1, $5, now(), now(), true)",
        )
        .bind(new_id("map"))
        .bind(&user.workspace.id)
        .bind(&source_product_id)
        .bind(&lp_id)
        .bind(&user.user.id)
        .execute(&state.pool)
        .await?;
        sqlx::query("update source_products set mapping_state = 'mapped' where workspace_id = $1 and id = $2")
            .bind(&user.workspace.id)
            .bind(&source_product_id)
            .execute(&state.pool)
            .await?;
        sqlx::query("update transactions set logical_product_id = $3 where workspace_id = $1 and source_product_id = $2")
            .bind(&user.workspace.id)
            .bind(&source_product_id)
            .bind(&lp_id)
            .execute(&state.pool)
            .await?;
        sqlx::query(
            "update subscriptions set logical_product_id = $3 where workspace_id = $1 and source_product_id = $2",
        )
        .bind(&user.workspace.id)
        .bind(&source_product_id)
        .bind(&lp_id)
        .execute(&state.pool)
        .await?;
        sqlx::query("update normalized_events set logical_product_id = $3 where workspace_id = $1 and source_product_id = $2")
            .bind(&user.workspace.id)
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
        where ne.workspace_id = $1
          and pm.workspace_id = $1
          and pm.source_product_id = ne.source_product_id
          and pm.active = true
          and ne.logical_product_id is null
        "#,
    )
    .bind(&user.workspace.id)
    .execute(&state.pool)
    .await?;
    Ok(())
}

async fn metric_warnings(state: &AppState, workspace_id: &str) -> ApiResult<Vec<String>> {
    let unmapped: i64 = sqlx::query_scalar("select count(*) from source_products where workspace_id = $1 and mapping_state = 'unmapped'")
        .bind(workspace_id)
        .fetch_one(&state.pool)
        .await?;
    let failed_jobs: i64 =
        sqlx::query_scalar("select count(*) from jobs where status in ('failed','dead')")
            .fetch_one(&state.pool)
            .await?;
    let source_count: i64 =
        sqlx::query_scalar("select count(*) from data_sources where workspace_id = $1")
            .bind(workspace_id)
            .fetch_one(&state.pool)
            .await?;
    let transaction_count: i64 =
        sqlx::query_scalar("select count(*) from transactions where workspace_id = $1")
            .bind(workspace_id)
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
    if let Some(value) = value {
        if !value.as_ref().is_empty() && value.as_ref() != "all" {
            query.push(" and ");
            query.push(column);
            query.push(" = ");
            query.push_bind(value.as_ref().to_string());
        }
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
    if let Some(value) = filters.get("source_event_type") {
        push_optional_filter(query, &format!("{alias}.source_event_type"), Some(value));
    }
    if let Some(from) = filters.get("from") {
        query.push(" and ");
        query.push(alias);
        query.push(".occurred_at::date >= ");
        query.push_bind(from.clone());
    }
    if let Some(to) = filters.get("to") {
        query.push(" and ");
        query.push(alias);
        query.push(".occurred_at::date <= ");
        query.push_bind(to.clone());
    }
    if let Some(q) = filters.get("q") {
        if !q.is_empty() {
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
    push_optional_filter(query, "t.customer_id", filters.get("customer_id"));
    if let Some(from) = filters.get("from") {
        query.push(" and t.purchase_time::date >= ");
        query.push_bind(from.clone());
    }
    if let Some(to) = filters.get("to") {
        query.push(" and t.purchase_time::date <= ");
        query.push_bind(to.clone());
    }
}

fn app_json(row: sqlx::postgres::PgRow) -> ApiResult<Value> {
    Ok(json!({
        "id": row.try_get::<String, _>("id")?,
        "name": row.try_get::<String, _>("name")?,
        "platform_bundle_id": row.try_get::<Option<String>, _>("platform_bundle_id")?,
        "apple_bundle_id": row.try_get::<Option<String>, _>("apple_bundle_id")?,
        "google_package_name": row.try_get::<Option<String>, _>("google_package_name")?,
        "default_currency": row.try_get::<Option<String>, _>("default_currency")?,
        "created_at": dt(row.try_get::<OffsetDateTime, _>("created_at")?),
        "updated_at": dt(row.try_get::<OffsetDateTime, _>("updated_at")?),
    }))
}

fn data_source_json(state: &AppState, row: sqlx::postgres::PgRow) -> ApiResult<Value> {
    let id: String = row.try_get("id")?;
    let source_type: String = row.try_get("source_type")?;
    let encrypted_credentials: Option<String> = row.try_get("encrypted_credentials")?;
    let has_credentials = encrypted_credentials.is_some();
    let has_webhook_secret = row
        .try_get::<Option<String>, _>("webhook_secret_hash")?
        .is_some();
    Ok(json!({
        "id": id,
        "workspace_id": row.try_get::<String, _>("workspace_id")?,
        "app_id": row.try_get::<Option<String>, _>("app_id")?,
        "app_name": row.try_get::<Option<String>, _>("app_name").ok().flatten(),
        "source_type": source_type,
        "name": row.try_get::<String, _>("name")?,
        "status": row.try_get::<String, _>("status")?,
        "has_credentials": has_credentials,
        "credential_keys": credential_keys(state, encrypted_credentials.as_deref()),
        "has_webhook_secret": has_webhook_secret,
        "last_event_at": opt_dt(row.try_get::<Option<OffsetDateTime>, _>("last_event_at")?),
        "last_sync_at": opt_dt(row.try_get::<Option<OffsetDateTime>, _>("last_sync_at")?),
        "last_error": row.try_get::<Option<String>, _>("last_error")?,
        "created_at": dt(row.try_get::<OffsetDateTime, _>("created_at")?),
        "updated_at": dt(row.try_get::<OffsetDateTime, _>("updated_at")?),
        "webhook_url": format!("{}/webhooks/{}/{}", state.config.base_url.trim_end_matches('/'), source_type.replace('_', "-"), row.try_get::<String, _>("id")?),
        "setup_checklist": setup_checklist(&source_type, row.try_get::<Option<OffsetDateTime>, _>("last_event_at")?.is_some(), has_webhook_secret, has_credentials)
    }))
}

fn setup_checklist(
    source_type: &str,
    has_event: bool,
    has_webhook_secret: bool,
    has_credentials: bool,
) -> Vec<Value> {
    match source_type {
        "revenuecat" => vec![
            json!({"key": "webhook_url", "label": "Paste Revtern webhook URL in RevenueCat", "done": true}),
            json!({"key": "secret", "label": "Configure Authorization/shared secret", "done": has_webhook_secret}),
            json!({"key": "first_event", "label": "Receive first RevenueCat event", "done": has_event}),
        ],
        "app_store" => vec![
            json!({"key": "notifications", "label": "Configure App Store Server Notification URL", "done": true}),
            json!({"key": "catch_up", "label": "Configure notification-history catch-up credentials", "done": has_credentials}),
            json!({"key": "signed_payload", "label": "Decode App Store signedPayload notifications", "done": has_event}),
        ],
        "google_play" => vec![
            json!({"key": "pubsub", "label": "Configure Pub/Sub push endpoint", "done": true}),
            json!({"key": "catch_up", "label": "Configure Pub/Sub pull credentials for missed RTDNs", "done": has_credentials}),
            json!({"key": "rtdn", "label": "Receive RTDN message", "done": has_event}),
        ],
        _ => vec![
            json!({"key": "webhook", "label": "Send events to the webhook endpoint", "done": has_event}),
            json!({"key": "catalog", "label": "Confirm discovered product catalog", "done": false}),
        ],
    }
}

fn credential_keys(state: &AppState, encrypted_credentials: Option<&str>) -> Vec<String> {
    let Some(encrypted_credentials) = encrypted_credentials else {
        return vec![];
    };
    crypto::decrypt_json(&state.config.secret_key, encrypted_credentials)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|value| {
            value
                .as_object()
                .map(|object| object.keys().cloned().collect())
        })
        .unwrap_or_default()
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
        "data_source_id": row.try_get::<String, _>("data_source_id")?,
        "data_source_name": row.try_get::<Option<String>, _>("data_source_name").ok().flatten(),
        "source_type": row.try_get::<String, _>("source_type")?,
        "source_event_id": row.try_get::<String, _>("source_event_id")?,
        "source_event_type": row.try_get::<Option<String>, _>("source_event_type")?,
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
        "updated_at": dt(row.try_get::<OffsetDateTime, _>("updated_at")?),
    }))
}

fn compact_event_json(row: sqlx::postgres::PgRow) -> ApiResult<Value> {
    Ok(json!({
        "id": row.try_get::<String, _>("id")?,
        "event_type": row.try_get::<String, _>("event_type")?,
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
