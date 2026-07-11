use anyhow::{Context, Result};
use base64::{Engine, engine::general_purpose::STANDARD};
use reqwest::{Client, Url};
use serde_json::{Map, Value, json};

use crate::catchup::{GOOGLE_ANDROID_PUBLISHER_SCOPE, google_access_token};

pub(crate) async fn processing_payload(
    source_type: &str,
    payload: &Value,
    credentials: Option<&Value>,
) -> Option<Value> {
    match source_type {
        "google_play" => Some(google_play_processing_payload(payload, credentials).await),
        _ => None,
    }
}

async fn google_play_processing_payload(payload: &Value, credentials: Option<&Value>) -> Value {
    let mut source = decode_pubsub_data(payload).unwrap_or_else(|| payload.clone());
    if !source.is_object() {
        return source;
    }

    if let Some(message) = payload.get("message")
        && let Some(object) = source.as_object_mut()
    {
        copy_string(message, object, "messageId", "pubsubMessageId");
        copy_string(message, object, "message_id", "pubsubMessageId");
        copy_string(message, object, "publishTime", "pubsubPublishTime");
    }

    let lookup = match google_play_purchase_lookup(&source, credentials).await {
        Ok(value) => value,
        Err(error) => json!({
            "status": "error",
            "environment": "unknown",
            "error": error.to_string(),
        }),
    };
    if let Some(object) = source.as_object_mut() {
        object.insert("googlePlayPurchaseLookup".to_string(), lookup);
    }
    source
}

async fn google_play_purchase_lookup(source: &Value, credentials: Option<&Value>) -> Result<Value> {
    if source.get("testNotification").is_some() {
        return Ok(json!({
            "status": "verified",
            "environment": "test",
            "source": "rtdn_test_notification",
        }));
    }

    let Some((kind, event)) = source
        .get("subscriptionNotification")
        .map(|value| ("subscription", value))
        .or_else(|| {
            source
                .get("oneTimeProductNotification")
                .map(|value| ("one_time_product", value))
        })
    else {
        return Ok(json!({
            "status": "not_applicable",
            "environment": "unknown",
        }));
    };

    let package_name = string_at(source, &["packageName", "package_name"])
        .context("Google Play RTDN packageName is required for purchase lookup")?;
    let token = string_at(event, &["purchaseToken", "purchase_token"])
        .context("Google Play RTDN purchaseToken is required for purchase lookup")?;

    let Some(credentials) = credentials else {
        return Ok(json!({
            "status": "not_configured",
            "environment": "unknown",
            "purchase_kind": kind,
            "error": "Android Publisher API credentials are not configured",
        }));
    };

    let access_token = android_publisher_access_token(credentials).await?;
    let purchase = if kind == "subscription" {
        fetch_subscription_purchase(&package_name, &token, &access_token).await?
    } else {
        fetch_product_purchase(&package_name, &token, &access_token).await?
    };
    Ok(summarize_purchase(kind, event, &purchase))
}

async fn android_publisher_access_token(credentials: &Value) -> Result<String> {
    if let Some(token) = string_at(
        credentials,
        &["android_publisher_access_token", "access_token"],
    ) {
        return Ok(token);
    }
    google_access_token(credentials, GOOGLE_ANDROID_PUBLISHER_SCOPE).await
}

async fn fetch_subscription_purchase(
    package_name: &str,
    token: &str,
    access_token: &str,
) -> Result<Value> {
    let mut url = Url::parse("https://androidpublisher.googleapis.com")?;
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("build Google Play subscription purchase URL"))?
        .extend([
            "androidpublisher",
            "v3",
            "applications",
            package_name,
            "purchases",
            "subscriptionsv2",
            "tokens",
            token,
        ]);
    fetch_google_json(url, access_token, "fetch Google Play subscription purchase").await
}

async fn fetch_product_purchase(
    package_name: &str,
    token: &str,
    access_token: &str,
) -> Result<Value> {
    let mut url = Url::parse("https://androidpublisher.googleapis.com")?;
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("build Google Play product purchase URL"))?
        .extend([
            "androidpublisher",
            "v3",
            "applications",
            package_name,
            "purchases",
            "productsv2",
            "tokens",
            token,
        ]);
    fetch_google_json(url, access_token, "fetch Google Play product purchase").await
}

async fn fetch_google_json(url: Url, access_token: &str, context: &'static str) -> Result<Value> {
    let response = Client::new()
        .get(url)
        .bearer_auth(access_token)
        .send()
        .await?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("{context} failed with {status}: {}", truncate_error(&body));
    }
    response.json().await.context(context)
}

fn summarize_purchase(kind: &str, event: &Value, purchase: &Value) -> Value {
    let is_test = if kind == "subscription" {
        purchase.get("testPurchase").is_some()
    } else {
        purchase.get("testPurchaseContext").is_some()
            || matches!(
                purchase.get("purchaseType").and_then(Value::as_i64),
                Some(0)
            )
    };
    let environment = if is_test { "test" } else { "production" };
    let line_item = purchase
        .pointer("/lineItems/0")
        .or_else(|| purchase.pointer("/productLineItem/0"));
    let price = line_item
        .and_then(|value| value.pointer("/autoRenewingPlan/recurringPrice"))
        .or_else(|| line_item.and_then(|value| value.pointer("/prepaidPlan/price")));

    json!({
        "status": "verified",
        "environment": environment,
        "purchase_kind": kind,
        "order_id": string_at(purchase, &["latestOrderId", "orderId"])
            .or_else(|| line_item.and_then(|value| string_at(value, &["latestSuccessfulOrderId"]))),
        "purchase_time": string_at(purchase, &["startTime", "purchaseCompletionTime", "purchaseTime"]),
        "start_time": string_at(purchase, &["startTime"]),
        "expiry_time": line_item.and_then(|value| string_at(value, &["expiryTime"])),
        "will_renew": line_item
            .and_then(|value| value.pointer("/autoRenewingPlan/autoRenewEnabled"))
            .and_then(Value::as_bool),
        "region_code": string_at(purchase, &["regionCode"]),
        "product_id": line_item
            .and_then(|value| string_at(value, &["productId"]))
            .or_else(|| string_at(event, &["subscriptionId", "sku", "productId", "product_id"])),
        "base_plan_id": line_item
            .and_then(|value| value.get("offerDetails"))
            .and_then(|value| string_at(value, &["basePlanId"])),
        "amount_minor": price.and_then(money_minor),
        "currency": price.and_then(|value| string_at(value, &["currencyCode"])),
        "obfuscated_external_account_id": purchase
            .pointer("/externalAccountIdentifiers/obfuscatedExternalAccountId")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| string_at(purchase, &["obfuscatedExternalAccountId"])),
    })
}

fn decode_pubsub_data(payload: &Value) -> Option<Value> {
    let encoded = payload.pointer("/message/data")?.as_str()?;
    let bytes = STANDARD.decode(encoded).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn copy_string(source: &Value, target: &mut Map<String, Value>, from: &str, to: &str) {
    if let Some(value) = source.get(from).and_then(Value::as_str) {
        target.insert(to.to_string(), json!(value));
    }
}

fn string_at(value: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = value
            .get(*key)
            .or_else(|| value.pointer(&format!("/{}", key.replace('.', "/"))))
        {
            match value {
                Value::String(text) if !text.trim().is_empty() => {
                    return Some(text.trim().to_string());
                }
                Value::Number(number) => return Some(number.to_string()),
                _ => {}
            }
        }
    }
    None
}

fn money_minor(value: &Value) -> Option<i64> {
    let units = string_at(value, &["units"])
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0);
    let nanos = value.get("nanos").and_then(Value::as_i64).unwrap_or(0);
    Some(units.saturating_mul(100) + nanos / 10_000_000)
}

fn truncate_error(body: &str) -> String {
    let text = body.replace(['\n', '\r'], " ");
    if text.len() > 240 {
        format!("{}...", &text[..240])
    } else {
        text
    }
}
