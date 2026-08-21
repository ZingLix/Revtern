use std::collections::{HashMap, HashSet};

use axum::{
    Json, Router,
    extract::{Query, State},
    response::Redirect,
    routing::get,
};
use axum_extra::extract::CookieJar;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header, jwk::JwkSet};
use revtern_core::{new_id, sha256_hex};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row};
use time::{Duration, OffsetDateTime};

use crate::{
    AppState,
    auth::{self, CurrentUser},
    config::{OidcConfig, RegistrationMode},
    crypto,
    error::{ApiError, ApiResult},
    routes::accept_invitation_in_transaction,
};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/auth/providers", get(auth_providers))
        .route("/api/auth/identities", get(auth_identities))
        .route("/api/auth/oidc/start", get(start_oidc_login))
        .route("/api/auth/oidc/link", get(start_oidc_link))
        .route("/api/auth/oidc/callback", get(oidc_callback))
        .with_state(state)
}

#[derive(Debug, Deserialize)]
struct OidcStartQuery {
    return_to: Option<String>,
    invite_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OidcCallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DiscoveryDocument {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    jwks_uri: String,
    token_endpoint_auth_methods_supported: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    id_token: String,
}

#[derive(Debug, Deserialize, serde::Serialize)]
struct IdTokenClaims {
    iss: String,
    sub: String,
    aud: Value,
    azp: Option<String>,
    exp: usize,
    iat: Option<usize>,
    nonce: Option<String>,
    email: Option<String>,
    email_verified: Option<bool>,
    name: Option<String>,
    preferred_username: Option<String>,
}

async fn auth_providers(State(state): State<AppState>) -> Json<Value> {
    let registration = match state.config.registration_mode {
        RegistrationMode::Closed => "closed",
        RegistrationMode::InviteOnly => "invite_only",
        RegistrationMode::Open => "open",
    };
    Json(json!({
        "local": { "enabled": state.config.auth_mode == crate::config::AuthMode::Local },
        "oidc": state.config.oidc.as_ref().map(|provider| json!({
            "enabled": true,
            "name": provider.name,
        })),
        "registration_mode": registration
    }))
}

async fn auth_identities(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Value>> {
    let has_local_password: bool = sqlx::query_scalar(
        "select password_hash is not null from users where id = $1 and status = 'active'",
    )
    .bind(&user.user.id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| ApiError::Unauthorized("account is not active".to_string()))?;
    let rows = sqlx::query(
        r#"
        select ai.id, ai.provider_id, ap.name as provider_name, ai.email,
               ai.email_verified, ai.last_authenticated_at, ai.created_at
        from auth_identities ai
        join auth_providers ap on ap.id = ai.provider_id
        where ai.user_id = $1
        order by ai.created_at asc
        "#,
    )
    .bind(&user.user.id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(json!({
        "has_local_password": has_local_password,
        "identities": rows.into_iter().map(|row| -> ApiResult<Value> {
            Ok(json!({
                "id": row.try_get::<String, _>("id")?,
                "provider_id": row.try_get::<String, _>("provider_id")?,
                "provider_name": row.try_get::<String, _>("provider_name")?,
                "email": row.try_get::<Option<String>, _>("email")?,
                "email_verified": row.try_get::<bool, _>("email_verified")?,
                "last_authenticated_at": row.try_get::<Option<OffsetDateTime>, _>("last_authenticated_at")?.map(format_datetime).transpose()?,
                "created_at": format_datetime(row.try_get::<OffsetDateTime, _>("created_at")?)?,
            }))
        }).collect::<ApiResult<Vec<_>>>()?
    })))
}

async fn start_oidc_login(
    State(state): State<AppState>,
    Query(query): Query<OidcStartQuery>,
) -> ApiResult<Redirect> {
    begin_oidc(&state, query, None).await
}

async fn start_oidc_link(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<OidcStartQuery>,
) -> ApiResult<Redirect> {
    begin_oidc(&state, query, Some(&user.user.id)).await
}

async fn begin_oidc(
    state: &AppState,
    query: OidcStartQuery,
    link_user_id: Option<&str>,
) -> ApiResult<Redirect> {
    let provider = oidc_config(state)?;
    ensure_provider(&state.pool, provider).await?;
    let discovery = discover(provider, &state.config.environment).await?;
    let state_token = auth::random_token();
    let nonce = auth::random_token();
    let verifier = auth::random_token();
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let return_to = safe_return_to(query.return_to.as_deref());
    let invitation_token_hash = query
        .invite_token
        .as_deref()
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(|token| sha256_hex(token.as_bytes()));

    if let Some(token_hash) = invitation_token_hash.as_deref() {
        let valid: bool = sqlx::query_scalar(
            "select exists(select 1 from app_invitations where token_hash = $1 and accepted_at is null and revoked_at is null and expires_at > now())",
        )
        .bind(token_hash)
        .fetch_one(&state.pool)
        .await?;
        if !valid {
            return Err(ApiError::NotFound(
                "invitation not found or expired".to_string(),
            ));
        }
    }

    sqlx::query(
        r#"
        insert into oidc_transactions (
          state_hash, provider_id, nonce_hash, encrypted_pkce_verifier,
          return_to, link_user_id, invitation_token_hash, expires_at, created_at
        ) values ($1, $2, $3, $4, $5, $6, $7, $8, now())
        "#,
    )
    .bind(sha256_hex(state_token.as_bytes()))
    .bind(&provider.provider_id)
    .bind(sha256_hex(nonce.as_bytes()))
    .bind(crypto::encrypt_json(
        &state.config.secret_key,
        verifier.as_bytes(),
    )?)
    .bind(&return_to)
    .bind(link_user_id)
    .bind(invitation_token_hash)
    .bind(OffsetDateTime::now_utc() + Duration::minutes(10))
    .execute(&state.pool)
    .await?;

    let mut authorization_url = reqwest::Url::parse(&discovery.authorization_endpoint)
        .map_err(|_| ApiError::invalid("OIDC authorization endpoint is invalid"))?;
    authorization_url
        .query_pairs_mut()
        .append_pair("client_id", &provider.client_id)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", &redirect_uri(state))
        .append_pair("scope", &provider.scopes)
        .append_pair("state", &state_token)
        .append_pair("nonce", &nonce)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256");
    Ok(Redirect::temporary(authorization_url.as_str()))
}

async fn oidc_callback(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(query): Query<OidcCallbackQuery>,
) -> ApiResult<(CookieJar, Redirect)> {
    if let Some(error) = query.error {
        return Err(ApiError::Unauthorized(format!(
            "OIDC sign-in failed: {}",
            query.error_description.unwrap_or(error)
        )));
    }
    let code = query
        .code
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::invalid("OIDC callback is missing code"))?;
    let state_token = query
        .state
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::invalid("OIDC callback is missing state"))?;
    let transaction = sqlx::query(
        r#"
        delete from oidc_transactions
        where state_hash = $1 and expires_at > now()
        returning provider_id, nonce_hash, encrypted_pkce_verifier, return_to,
                  link_user_id, invitation_token_hash
        "#,
    )
    .bind(sha256_hex(state_token.as_bytes()))
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| ApiError::Unauthorized("OIDC state is invalid or expired".to_string()))?;

    let provider = oidc_config(&state)?;
    let transaction_provider_id: String = transaction.try_get("provider_id")?;
    if transaction_provider_id != provider.provider_id {
        return Err(ApiError::Unauthorized(
            "OIDC provider does not match the login transaction".to_string(),
        ));
    }
    let discovery = discover(provider, &state.config.environment).await?;
    let verifier = String::from_utf8(crypto::decrypt_json(
        &state.config.secret_key,
        transaction.try_get("encrypted_pkce_verifier")?,
    )?)
    .map_err(|_| ApiError::Unauthorized("OIDC PKCE verifier is invalid".to_string()))?;
    let token = exchange_code(&discovery, provider, code, &verifier, &redirect_uri(&state)).await?;
    let claims = validate_id_token(&discovery, provider, &token.id_token).await?;
    let expected_nonce_hash: String = transaction.try_get("nonce_hash")?;
    let nonce = claims
        .nonce
        .as_deref()
        .ok_or_else(|| ApiError::Unauthorized("OIDC ID token is missing nonce".to_string()))?;
    if sha256_hex(nonce.as_bytes()) != expected_nonce_hash {
        return Err(ApiError::Unauthorized(
            "OIDC nonce validation failed".to_string(),
        ));
    }

    let link_user_id: Option<String> = transaction.try_get("link_user_id")?;
    let invitation_token_hash: Option<String> = transaction.try_get("invitation_token_hash")?;
    let (user_id, identity_id, accepted_app_id) = resolve_identity(
        &state,
        provider,
        &claims,
        link_user_id.as_deref(),
        invitation_token_hash.as_deref(),
    )
    .await?;
    let jar = auth::create_session_with_identity(
        &state.pool,
        &state.config,
        &user_id,
        Some(&identity_id),
        "oidc",
        jar,
    )
    .await?;
    let return_to: String = transaction.try_get("return_to")?;
    let return_to = accepted_app_id
        .map(|app_id| format!("/?app_id={app_id}"))
        .unwrap_or_else(|| safe_return_to(Some(&return_to)));
    Ok((jar, Redirect::to(&return_to)))
}

async fn discover(provider: &OidcConfig, environment: &str) -> ApiResult<DiscoveryDocument> {
    let url = format!(
        "{}/.well-known/openid-configuration",
        provider.issuer.trim_end_matches('/')
    );
    let discovery = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .map_err(|error| ApiError::Unauthorized(format!("OIDC discovery failed: {error}")))?
        .error_for_status()
        .map_err(|error| ApiError::Unauthorized(format!("OIDC discovery failed: {error}")))?
        .json::<DiscoveryDocument>()
        .await
        .map_err(|error| ApiError::Unauthorized(format!("OIDC discovery is invalid: {error}")))?;
    if discovery.issuer.trim_end_matches('/') != provider.issuer.trim_end_matches('/') {
        return Err(ApiError::Unauthorized(
            "OIDC discovery issuer does not match configuration".to_string(),
        ));
    }
    if environment != "development"
        && [
            &discovery.authorization_endpoint,
            &discovery.token_endpoint,
            &discovery.jwks_uri,
        ]
        .iter()
        .any(|endpoint| !endpoint.starts_with("https://"))
    {
        return Err(ApiError::Unauthorized(
            "OIDC endpoints must use HTTPS".to_string(),
        ));
    }
    Ok(discovery)
}

async fn exchange_code(
    discovery: &DiscoveryDocument,
    provider: &OidcConfig,
    code: &str,
    verifier: &str,
    redirect_uri: &str,
) -> ApiResult<TokenResponse> {
    let use_basic = discovery
        .token_endpoint_auth_methods_supported
        .as_ref()
        .is_some_and(|methods| {
            methods.iter().any(|method| method == "client_secret_basic")
                && !methods.iter().any(|method| method == "client_secret_post")
        });
    let mut form = HashMap::from([
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", provider.client_id.as_str()),
        ("code_verifier", verifier),
    ]);
    if !use_basic {
        form.insert("client_secret", provider.client_secret.as_str());
    }
    let client = reqwest::Client::new();
    let request = client.post(&discovery.token_endpoint).form(&form);
    let request = if use_basic {
        request.basic_auth(&provider.client_id, Some(&provider.client_secret))
    } else {
        request
    };
    request
        .send()
        .await
        .map_err(|error| ApiError::Unauthorized(format!("OIDC token exchange failed: {error}")))?
        .error_for_status()
        .map_err(|error| ApiError::Unauthorized(format!("OIDC token exchange failed: {error}")))?
        .json::<TokenResponse>()
        .await
        .map_err(|error| ApiError::Unauthorized(format!("OIDC token response is invalid: {error}")))
}

async fn validate_id_token(
    discovery: &DiscoveryDocument,
    provider: &OidcConfig,
    id_token: &str,
) -> ApiResult<IdTokenClaims> {
    let header = decode_header(id_token)
        .map_err(|_| ApiError::Unauthorized("OIDC ID token header is invalid".to_string()))?;
    if !matches!(
        header.alg,
        Algorithm::RS256
            | Algorithm::RS384
            | Algorithm::RS512
            | Algorithm::PS256
            | Algorithm::PS384
            | Algorithm::PS512
            | Algorithm::ES256
            | Algorithm::ES384
            | Algorithm::EdDSA
    ) {
        return Err(ApiError::Unauthorized(
            "OIDC ID token uses an unsupported signing algorithm".to_string(),
        ));
    }
    let jwks = reqwest::Client::new()
        .get(&discovery.jwks_uri)
        .send()
        .await
        .map_err(|error| ApiError::Unauthorized(format!("OIDC JWKS fetch failed: {error}")))?
        .error_for_status()
        .map_err(|error| ApiError::Unauthorized(format!("OIDC JWKS fetch failed: {error}")))?
        .json::<JwkSet>()
        .await
        .map_err(|error| ApiError::Unauthorized(format!("OIDC JWKS is invalid: {error}")))?;
    let jwk = match header.kid.as_deref() {
        Some(kid) => jwks.find(kid),
        None if jwks.keys.len() == 1 => jwks.keys.first(),
        None => None,
    }
    .ok_or_else(|| ApiError::Unauthorized("OIDC signing key was not found".to_string()))?;
    let key = DecodingKey::from_jwk(jwk)
        .map_err(|_| ApiError::Unauthorized("OIDC signing key is invalid".to_string()))?;
    let mut validation = Validation::new(header.alg);
    validation.set_audience(&[provider.client_id.as_str()]);
    validation.set_issuer(&[discovery.issuer.as_str()]);
    validation.required_spec_claims = HashSet::from([
        "exp".to_string(),
        "iss".to_string(),
        "aud".to_string(),
        "sub".to_string(),
    ]);
    let claims = decode::<IdTokenClaims>(id_token, &key, &validation)
        .map(|token| token.claims)
        .map_err(|error| {
            ApiError::Unauthorized(format!("OIDC ID token validation failed: {error}"))
        })?;
    let audience_count = match &claims.aud {
        Value::String(_) => 1,
        Value::Array(values) => values.len(),
        _ => 0,
    };
    if audience_count > 1 && claims.azp.as_deref() != Some(provider.client_id.as_str()) {
        return Err(ApiError::Unauthorized(
            "OIDC ID token with multiple audiences must identify this client as azp".to_string(),
        ));
    }
    if claims
        .azp
        .as_deref()
        .is_some_and(|azp| azp != provider.client_id)
    {
        return Err(ApiError::Unauthorized(
            "OIDC ID token azp does not match this client".to_string(),
        ));
    }
    Ok(claims)
}

async fn resolve_identity(
    state: &AppState,
    provider: &OidcConfig,
    claims: &IdTokenClaims,
    link_user_id: Option<&str>,
    invitation_token_hash: Option<&str>,
) -> ApiResult<(String, String, Option<String>)> {
    let existing = sqlx::query(
        "select ai.id, ai.user_id from auth_identities ai join users u on u.id = ai.user_id where ai.provider_id = $1 and ai.subject = $2 and u.status = 'active'",
    )
    .bind(&provider.provider_id)
    .bind(&claims.sub)
    .fetch_optional(&state.pool)
    .await?;
    if let Some(row) = existing {
        let identity_id: String = row.try_get("id")?;
        let user_id: String = row.try_get("user_id")?;
        if link_user_id.is_some_and(|link_user_id| link_user_id != user_id) {
            return Err(ApiError::Conflict(
                "this OIDC identity is already linked to another account".to_string(),
            ));
        }
        sqlx::query(
            "update auth_identities set email = $2, email_verified = $3, claims = $4, last_authenticated_at = now(), updated_at = now() where id = $1",
        )
        .bind(&identity_id)
        .bind(claims.email.as_deref())
        .bind(claims.email_verified.unwrap_or(false))
        .bind(serde_json::to_value(claims)?)
        .execute(&state.pool)
        .await?;
        let accepted_app_id =
            accept_oidc_invitation(state, invitation_token_hash, &user_id).await?;
        return Ok((user_id, identity_id, accepted_app_id));
    }

    if let Some(user_id) = link_user_id {
        let identity_id = insert_identity(&state.pool, provider, claims, user_id).await?;
        sqlx::query(
            "insert into audit_events (id, actor_user_id, action, target_type, target_id, metadata, created_at) values ($1, $2, 'auth.identity.linked', 'auth_identity', $3, $4, now())",
        )
        .bind(new_id("aud"))
        .bind(user_id)
        .bind(&identity_id)
        .bind(json!({ "provider_id": provider.provider_id }))
        .execute(&state.pool)
        .await?;
        let accepted_app_id = accept_oidc_invitation(state, invitation_token_hash, user_id).await?;
        return Ok((user_id.to_string(), identity_id, accepted_app_id));
    }
    if !claims.email_verified.unwrap_or(false) {
        return Err(ApiError::Forbidden(
            "a verified email claim is required to create an account".to_string(),
        ));
    }
    let email = claims
        .email
        .as_deref()
        .map(str::trim)
        .filter(|email| !email.is_empty())
        .ok_or_else(|| ApiError::Forbidden("OIDC provider did not return an email".to_string()))?
        .to_ascii_lowercase();
    let email_exists: bool =
        sqlx::query_scalar("select exists(select 1 from users where email = $1)")
            .bind(&email)
            .fetch_one(&state.pool)
            .await?;
    if email_exists {
        return Err(ApiError::Conflict(
            "an account with this email already exists; sign in locally and link OIDC from settings"
                .to_string(),
        ));
    }
    match state.config.registration_mode {
        RegistrationMode::Closed => {
            return Err(ApiError::Forbidden("registration is closed".to_string()));
        }
        RegistrationMode::InviteOnly if invitation_token_hash.is_none() => {
            return Err(ApiError::Forbidden(
                "a valid invitation is required".to_string(),
            ));
        }
        _ => {}
    }

    create_oidc_user(state, provider, claims, &email, invitation_token_hash).await
}

async fn create_oidc_user(
    state: &AppState,
    provider: &OidcConfig,
    claims: &IdTokenClaims,
    email: &str,
    invitation_token_hash: Option<&str>,
) -> ApiResult<(String, String, Option<String>)> {
    let user_id = new_id("usr");
    let workspace_id = new_id("wsp");
    let identity_id = new_id("idn");
    let display_name = claims
        .name
        .as_deref()
        .or(claims.preferred_username.as_deref())
        .unwrap_or(email);
    let mut tx = state.pool.begin().await?;
    sqlx::query(
        "insert into users (id, email, password_hash, display_name, role, status, created_at, updated_at) values ($1, $2, null, $3, 'owner', 'active', now(), now())",
    )
    .bind(&user_id)
    .bind(email)
    .bind(display_name)
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
    insert_identity_in_transaction(&mut tx, &identity_id, provider, claims, &user_id).await?;
    let accepted_app_id = if let Some(token_hash) = invitation_token_hash {
        Some(accept_invitation_in_transaction(&mut tx, token_hash, &user_id, email).await?)
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
    .bind(json!({ "auth_method": "oidc", "provider_id": provider.provider_id }))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((user_id, identity_id, accepted_app_id))
}

async fn accept_oidc_invitation(
    state: &AppState,
    invitation_token_hash: Option<&str>,
    user_id: &str,
) -> ApiResult<Option<String>> {
    let Some(token_hash) = invitation_token_hash else {
        return Ok(None);
    };
    let email: String =
        sqlx::query_scalar("select email from users where id = $1 and status = 'active'")
            .bind(user_id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(|| ApiError::Unauthorized("account is not active".to_string()))?;
    let mut tx = state.pool.begin().await?;
    let app_id = accept_invitation_in_transaction(&mut tx, token_hash, user_id, &email).await?;
    tx.commit().await?;
    Ok(Some(app_id))
}

async fn insert_identity(
    pool: &sqlx::PgPool,
    provider: &OidcConfig,
    claims: &IdTokenClaims,
    user_id: &str,
) -> ApiResult<String> {
    let identity_id = new_id("idn");
    sqlx::query(
        r#"
        insert into auth_identities (
          id, user_id, provider_id, subject, email, email_verified, claims,
          last_authenticated_at, created_at, updated_at
        ) values ($1, $2, $3, $4, $5, $6, $7, now(), now(), now())
        "#,
    )
    .bind(&identity_id)
    .bind(user_id)
    .bind(&provider.provider_id)
    .bind(&claims.sub)
    .bind(claims.email.as_deref())
    .bind(claims.email_verified.unwrap_or(false))
    .bind(serde_json::to_value(claims)?)
    .execute(pool)
    .await?;
    Ok(identity_id)
}

async fn insert_identity_in_transaction(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    identity_id: &str,
    provider: &OidcConfig,
    claims: &IdTokenClaims,
    user_id: &str,
) -> ApiResult<()> {
    sqlx::query(
        r#"
        insert into auth_identities (
          id, user_id, provider_id, subject, email, email_verified, claims,
          last_authenticated_at, created_at, updated_at
        ) values ($1, $2, $3, $4, $5, $6, $7, now(), now(), now())
        "#,
    )
    .bind(identity_id)
    .bind(user_id)
    .bind(&provider.provider_id)
    .bind(&claims.sub)
    .bind(claims.email.as_deref())
    .bind(claims.email_verified.unwrap_or(false))
    .bind(serde_json::to_value(claims)?)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn ensure_provider(pool: &sqlx::PgPool, provider: &OidcConfig) -> ApiResult<()> {
    sqlx::query(
        r#"
        insert into auth_providers (
          id, provider_type, name, issuer, client_id, scopes, enabled, created_at, updated_at
        ) values ($1, 'oidc', $2, $3, $4, $5, true, now(), now())
        on conflict (id) do update
          set name = excluded.name,
              issuer = excluded.issuer,
              client_id = excluded.client_id,
              scopes = excluded.scopes,
              enabled = true,
              updated_at = now()
        "#,
    )
    .bind(&provider.provider_id)
    .bind(&provider.name)
    .bind(&provider.issuer)
    .bind(&provider.client_id)
    .bind(&provider.scopes)
    .execute(pool)
    .await?;
    Ok(())
}

fn oidc_config(state: &AppState) -> ApiResult<&OidcConfig> {
    state
        .config
        .oidc
        .as_ref()
        .ok_or_else(|| ApiError::NotFound("OIDC is not configured".to_string()))
}

fn redirect_uri(state: &AppState) -> String {
    format!(
        "{}/api/auth/oidc/callback",
        state.config.base_url.trim_end_matches('/')
    )
}

fn safe_return_to(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|value| value.starts_with('/') && !value.starts_with("//"))
        .unwrap_or("/")
        .to_string()
}

fn format_datetime(value: OffsetDateTime) -> ApiResult<String> {
    value
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|error| anyhow::anyhow!(error).into())
}

#[cfg(test)]
mod tests {
    use super::safe_return_to;

    #[test]
    fn only_allows_local_return_paths() {
        assert_eq!(safe_return_to(Some("/settings")), "/settings");
        assert_eq!(safe_return_to(Some("https://evil.example")), "/");
        assert_eq!(safe_return_to(Some("//evil.example")), "/");
    }
}
