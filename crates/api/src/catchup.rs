use anyhow::{Context, Result};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use time::OffsetDateTime;

#[derive(Debug, Clone)]
pub struct CatchUpWindow {
    pub from: OffsetDateTime,
    pub to: OffsetDateTime,
    pub limit: usize,
    pub cursor: Option<String>,
}

#[derive(Debug)]
pub struct CatchUpBatch {
    pub payloads: Vec<Value>,
    pub next_cursor: Option<String>,
    pub ack: Option<CatchUpAck>,
}

#[derive(Debug)]
pub enum CatchUpAck {
    GooglePubSub {
        subscription: String,
        access_token: String,
        ack_ids: Vec<String>,
    },
}

pub async fn fetch_webhook_notifications(
    source_type: &str,
    credentials: &Value,
    window: &CatchUpWindow,
) -> Result<CatchUpBatch> {
    match source_type {
        "app_store" => fetch_app_store_notifications(credentials, window).await,
        "google_play" => fetch_google_pubsub_messages(credentials, window).await,
        _ => anyhow::bail!("webhook catch-up is only supported for App Store and Google Play"),
    }
}

pub async fn acknowledge_batch(ack: CatchUpAck) -> Result<()> {
    match ack {
        CatchUpAck::GooglePubSub {
            subscription,
            access_token,
            ack_ids,
        } => {
            if ack_ids.is_empty() {
                return Ok(());
            }
            let client = Client::new();
            let url = format!(
                "https://pubsub.googleapis.com/v1/{}:acknowledge",
                subscription
            );
            client
                .post(url)
                .bearer_auth(access_token)
                .json(&json!({ "ackIds": ack_ids }))
                .send()
                .await?
                .error_for_status()
                .context("acknowledge Google Pub/Sub webhook messages")?;
            Ok(())
        }
    }
}

async fn fetch_app_store_notifications(
    credentials: &Value,
    window: &CatchUpWindow,
) -> Result<CatchUpBatch> {
    let issuer_id = required_string(credentials, "issuer_id")?;
    let key_id = required_string(credentials, "key_id")?;
    let private_key = required_string(credentials, "private_key")?.replace("\\n", "\n");
    let bundle_id = required_string(credentials, "bundle_id")?;
    let environment = optional_string(credentials, "environment").unwrap_or("production");
    let host = if environment.eq_ignore_ascii_case("sandbox") {
        "https://api.storekit-sandbox.itunes.apple.com"
    } else {
        "https://api.storekit.itunes.apple.com"
    };
    let token = app_store_token(&issuer_id, &key_id, &private_key, &bundle_id)?;
    let client = Client::new();
    let mut body = json!({
        "startDate": window.from.unix_timestamp() * 1000,
        "endDate": window.to.unix_timestamp() * 1000,
    });
    if let Some(cursor) = &window.cursor {
        body["paginationToken"] = json!(cursor);
    }

    let response: AppStoreNotificationHistoryResponse = client
        .post(format!("{host}/inApps/v1/notifications/history"))
        .bearer_auth(token)
        .json(&body)
        .send()
        .await?
        .error_for_status()
        .context("pull App Store notification history")?
        .json()
        .await
        .context("decode App Store notification history")?;

    let mut payloads = Vec::with_capacity(response.notification_history.len());
    for item in response.notification_history.into_iter().take(window.limit) {
        payloads.push(json!({ "signedPayload": item.signed_payload }));
    }

    Ok(CatchUpBatch {
        payloads,
        next_cursor: response.pagination_token,
        ack: None,
    })
}

async fn fetch_google_pubsub_messages(
    credentials: &Value,
    window: &CatchUpWindow,
) -> Result<CatchUpBatch> {
    let subscription = required_string_any(credentials, &["pubsub_subscription", "subscription"])?;
    if subscription.contains("/topics/") {
        anyhow::bail!(
            "pubsub_subscription must be a subscription path like projects/PROJECT_ID/subscriptions/SUBSCRIPTION_ID, not a topic path"
        );
    }
    let access_token = if let Some(token) = optional_string(credentials, "access_token") {
        token.to_string()
    } else {
        google_access_token(credentials).await?
    };
    let client = Client::new();
    let max_messages = window.limit.clamp(1, 100);
    let response: GooglePullResponse = client
        .post(format!(
            "https://pubsub.googleapis.com/v1/{subscription}:pull"
        ))
        .bearer_auth(&access_token)
        .json(&json!({ "maxMessages": max_messages }))
        .send()
        .await?
        .error_for_status()
        .context("pull Google Pub/Sub RTDN backlog")?
        .json()
        .await
        .context("decode Google Pub/Sub pull response")?;

    let mut payloads = Vec::with_capacity(response.received_messages.len());
    let mut ack_ids = Vec::with_capacity(response.received_messages.len());
    for received in response.received_messages {
        if let Some(ack_id) = received.ack_id {
            ack_ids.push(ack_id);
        }
        let message = received.message;
        payloads.push(json!({
            "message": {
                "data": message.data,
                "messageId": message.message_id,
                "publishTime": message.publish_time,
                "attributes": message.attributes,
            },
            "subscription": subscription,
        }));
    }

    Ok(CatchUpBatch {
        payloads,
        next_cursor: None,
        ack: Some(CatchUpAck::GooglePubSub {
            subscription,
            access_token,
            ack_ids,
        }),
    })
}

fn app_store_token(
    issuer_id: &str,
    key_id: &str,
    private_key: &str,
    bundle_id: &str,
) -> Result<String> {
    #[derive(Serialize)]
    struct Claims<'a> {
        iss: &'a str,
        iat: i64,
        exp: i64,
        aud: &'a str,
        bid: &'a str,
    }

    let now = OffsetDateTime::now_utc().unix_timestamp();
    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some(key_id.to_string());
    let key =
        EncodingKey::from_ec_pem(private_key.as_bytes()).context("parse App Store private key")?;
    encode(
        &header,
        &Claims {
            iss: issuer_id,
            iat: now,
            exp: now + 20 * 60,
            aud: "appstoreconnect-v1",
            bid: bundle_id,
        },
        &key,
    )
    .context("sign App Store Server API token")
}

async fn google_access_token(credentials: &Value) -> Result<String> {
    let service_account = service_account_json(credentials)?;
    let client_email = required_string(&service_account, "client_email")?;
    let private_key = required_string(&service_account, "private_key")?.replace("\\n", "\n");
    let token_uri = optional_string(&service_account, "token_uri")
        .unwrap_or("https://oauth2.googleapis.com/token");

    #[derive(Serialize)]
    struct Claims<'a> {
        iss: &'a str,
        scope: &'a str,
        aud: &'a str,
        iat: i64,
        exp: i64,
    }

    let now = OffsetDateTime::now_utc().unix_timestamp();
    let key = EncodingKey::from_rsa_pem(private_key.as_bytes())
        .context("parse Google service account private key")?;
    let assertion = encode(
        &Header::new(Algorithm::RS256),
        &Claims {
            iss: &client_email,
            scope: "https://www.googleapis.com/auth/pubsub",
            aud: token_uri,
            iat: now,
            exp: now + 3600,
        },
        &key,
    )
    .context("sign Google service account assertion")?;

    #[derive(Deserialize)]
    struct TokenResponse {
        access_token: String,
    }

    let response: TokenResponse = Client::new()
        .post(token_uri)
        .form(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
            ("assertion", assertion.as_str()),
        ])
        .send()
        .await?
        .error_for_status()
        .context("exchange Google service account assertion")?
        .json()
        .await
        .context("decode Google token response")?;
    Ok(response.access_token)
}

fn service_account_json(credentials: &Value) -> Result<Value> {
    let value = credentials
        .get("service_account_json")
        .context("service_account_json is required for Google Pub/Sub catch-up")?;
    if let Some(text) = value.as_str() {
        serde_json::from_str(text).context("parse service_account_json")
    } else {
        Ok(value.clone())
    }
}

fn required_string(value: &Value, key: &str) -> Result<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .with_context(|| format!("{key} is required"))
}

fn required_string_any(value: &Value, keys: &[&str]) -> Result<String> {
    for key in keys {
        if let Some(value) = optional_string(value, key) {
            return Ok(value.to_string());
        }
    }
    anyhow::bail!("{} is required", keys.join(" or "))
}

fn optional_string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
}

#[derive(Debug, Deserialize)]
struct AppStoreNotificationHistoryResponse {
    #[serde(default, rename = "paginationToken")]
    pagination_token: Option<String>,
    #[serde(default, rename = "notificationHistory")]
    notification_history: Vec<AppStoreNotificationHistoryItem>,
}

#[derive(Debug, Deserialize)]
struct AppStoreNotificationHistoryItem {
    #[serde(rename = "signedPayload")]
    signed_payload: String,
}

#[derive(Debug, Deserialize)]
struct GooglePullResponse {
    #[serde(default, rename = "receivedMessages")]
    received_messages: Vec<GoogleReceivedMessage>,
}

#[derive(Debug, Deserialize)]
struct GoogleReceivedMessage {
    #[serde(default, rename = "ackId")]
    ack_id: Option<String>,
    #[serde(default)]
    message: GooglePubSubMessage,
}

#[derive(Debug, Deserialize, Default)]
struct GooglePubSubMessage {
    #[serde(default)]
    data: Option<String>,
    #[serde(default, rename = "messageId")]
    message_id: Option<String>,
    #[serde(default, rename = "publishTime")]
    publish_time: Option<String>,
    #[serde(default)]
    attributes: Option<Value>,
}
