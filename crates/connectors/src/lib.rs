use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use revtern_core::{
    infer_billing_period, infer_product_kind, normalize_money_minor, parse_time, source_product_key,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedEvent {
    pub source_event_id: String,
    pub source_event_type: String,
    pub environment: String,
    pub source_app_id: Option<String>,
    pub source_product_key: Option<String>,
    pub external_product_id: Option<String>,
    pub external_base_plan_id: Option<String>,
    pub external_offer_id: Option<String>,
    pub display_name: Option<String>,
    pub product_kind: String,
    pub billing_period: String,
    pub platform: Option<String>,
    pub normalized_event_type: Option<String>,
    pub amount_minor: Option<i64>,
    pub currency: Option<String>,
    pub country: Option<String>,
    pub customer_key: Option<String>,
    pub transaction_key: Option<String>,
    pub original_transaction_key: Option<String>,
    pub subscription_key: Option<String>,
    pub occurred_at: OffsetDateTime,
    pub purchase_time: Option<OffsetDateTime>,
    pub period_start: Option<OffsetDateTime>,
    pub period_end: Option<OffsetDateTime>,
    pub will_renew: Option<bool>,
    pub warnings: Vec<String>,
}

pub fn extract_event(source_type: &str, payload: &Value, fallback_id: &str) -> ExtractedEvent {
    match source_type {
        "revenuecat" => extract_revenuecat(payload, fallback_id),
        "custom_api" => extract_custom(payload, fallback_id),
        "app_store" => extract_app_store(payload, fallback_id),
        "google_play" => extract_google_play(payload, fallback_id),
        "stripe" => extract_stripe(payload, fallback_id),
        "paddle" => extract_paddle(payload, fallback_id),
        "csv" => extract_custom(payload, fallback_id),
        _ => extract_generic(source_type, payload, fallback_id),
    }
}

fn extract_revenuecat(payload: &Value, fallback_id: &str) -> ExtractedEvent {
    let event = payload.get("event").unwrap_or(payload);
    let event_type_raw = pick_string(event, &["type", "event_type", "notification_type"])
        .unwrap_or_else(|| "UNKNOWN".to_string());
    let normalized_event_type = map_revenuecat_type(&event_type_raw);
    let product_id = pick_string(
        event,
        &[
            "product_id",
            "product_identifier",
            "store_product_identifier",
            "presented_offering_id",
        ],
    );
    let source_app_id = pick_string(event, &["app_id", "project_id", "store"]);
    let platform = pick_string(event, &["store", "platform"]).map(normalize_platform);
    let occurred_at = pick_time(
        event,
        &[
            "event_timestamp_ms",
            "purchased_at_ms",
            "expiration_at_ms",
            "event_timestamp",
            "occurred_at",
        ],
    );
    let product_kind = pick_string(event, &["product_kind"]).unwrap_or_else(|| {
        infer_product_kind(&event_type_raw, product_id.as_deref().unwrap_or(""))
    });
    let billing_period = pick_string(event, &["period", "billing_period"])
        .unwrap_or_else(|| infer_billing_period(product_id.as_deref().unwrap_or("")));
    let source_product_key = product_id
        .as_deref()
        .map(|id| source_product_key("revenuecat", source_app_id.as_deref(), id, None));

    ExtractedEvent {
        source_event_id: pick_string(event, &["id", "event_id", "transaction_id"])
            .unwrap_or_else(|| fallback_id.to_string()),
        source_event_type: event_type_raw,
        environment: event_environment(event).unwrap_or_else(|| "production".to_string()),
        source_app_id,
        source_product_key,
        external_product_id: product_id.clone(),
        external_base_plan_id: None,
        external_offer_id: pick_string(event, &["offer_id", "package_id"]),
        display_name: product_id,
        product_kind,
        billing_period,
        platform,
        normalized_event_type,
        amount_minor: pick_money(
            event,
            &[
                "price_in_purchased_currency",
                "price",
                "amount",
                "amount_minor",
            ],
        ),
        currency: pick_string(event, &["currency", "currency_code", "price_currency"]),
        country: pick_string(event, &["country_code", "country"]),
        customer_key: pick_string(event, &["app_user_id", "subscriber_id", "customer_id"]),
        transaction_key: pick_string(event, &["transaction_id", "store_transaction_id"]),
        original_transaction_key: pick_string(
            event,
            &["original_transaction_id", "original_store_transaction_id"],
        ),
        subscription_key: pick_string(
            event,
            &["original_transaction_id", "subscription_id", "app_user_id"],
        ),
        occurred_at,
        purchase_time: pick_optional_time(event, &["purchased_at_ms", "purchased_at"]),
        period_start: pick_optional_time(event, &["purchased_at_ms", "period_start"]),
        period_end: pick_optional_time(event, &["expiration_at_ms", "expires_at_ms", "period_end"]),
        will_renew: pick_bool(event, &["will_renew"]),
        warnings: vec![],
    }
}

fn extract_custom(payload: &Value, fallback_id: &str) -> ExtractedEvent {
    let event_type_raw =
        pick_string(payload, &["event_type", "type"]).unwrap_or_else(|| "purchase".to_string());
    let product_id = pick_string(payload, &["product_id", "source_product_id", "sku"]);
    let source_app_id = pick_string(payload, &["app_id", "bundle_id", "package_name"]);
    let external_base_plan_id = pick_string(payload, &["base_plan_id", "external_base_plan_id"]);
    let source_product_key = product_id.as_deref().map(|id| {
        source_product_key(
            "custom_api",
            source_app_id.as_deref(),
            id,
            external_base_plan_id.as_deref(),
        )
    });
    let normalized_event_type = Some(map_common_type(&event_type_raw));

    ExtractedEvent {
        source_event_id: pick_string(payload, &["event_id", "id", "transaction_id"])
            .unwrap_or_else(|| fallback_id.to_string()),
        source_event_type: event_type_raw.clone(),
        environment: event_environment(payload).unwrap_or_else(|| "unknown".to_string()),
        source_app_id,
        source_product_key,
        external_product_id: product_id.clone(),
        external_base_plan_id,
        external_offer_id: pick_string(payload, &["offer_id"]),
        display_name: pick_string(payload, &["display_name", "product_name"])
            .or(product_id.clone()),
        product_kind: pick_string(payload, &["product_kind"]).unwrap_or_else(|| {
            infer_product_kind(&event_type_raw, product_id.as_deref().unwrap_or(""))
        }),
        billing_period: pick_string(payload, &["billing_period"])
            .unwrap_or_else(|| infer_billing_period(product_id.as_deref().unwrap_or(""))),
        platform: pick_string(payload, &["platform"]).map(normalize_platform),
        normalized_event_type,
        amount_minor: pick_money(payload, &["amount_minor", "amount", "price"]),
        currency: pick_string(payload, &["currency"]),
        country: pick_string(payload, &["country", "country_code"]),
        customer_key: pick_string(payload, &["customer_id", "customer_key", "app_user_id"]),
        transaction_key: pick_string(payload, &["transaction_id", "transaction_key"]),
        original_transaction_key: pick_string(
            payload,
            &["original_transaction_id", "original_transaction_key"],
        ),
        subscription_key: pick_string(
            payload,
            &[
                "subscription_id",
                "subscription_key",
                "original_transaction_id",
            ],
        ),
        occurred_at: pick_time(payload, &["occurred_at", "event_time", "purchase_time"]),
        purchase_time: pick_optional_time(
            payload,
            &["purchase_time", "purchased_at", "occurred_at"],
        ),
        period_start: pick_optional_time(payload, &["current_period_start", "period_start"]),
        period_end: pick_optional_time(
            payload,
            &["current_period_end", "period_end", "expires_at"],
        ),
        will_renew: pick_bool(payload, &["will_renew", "auto_renewing"]),
        warnings: vec![],
    }
}

fn extract_app_store(payload: &Value, fallback_id: &str) -> ExtractedEvent {
    let decoded_signed_payload = payload
        .get("signedPayload")
        .and_then(Value::as_str)
        .and_then(decode_jws_payload);
    let notification = decoded_signed_payload.as_ref().unwrap_or(payload);
    let data = notification
        .get("data")
        .or_else(|| notification.get("signedPayloadData"))
        .unwrap_or(notification);
    let decoded_transaction = data
        .get("signedTransactionInfo")
        .and_then(Value::as_str)
        .and_then(decode_jws_payload);
    let decoded_renewal = data
        .get("signedRenewalInfo")
        .and_then(Value::as_str)
        .and_then(decode_jws_payload);
    let transaction = data
        .get("transactionInfo")
        .or_else(|| data.get("transaction"))
        .or(decoded_transaction.as_ref())
        .or(decoded_renewal.as_ref())
        .unwrap_or(data);
    let event_type_raw = pick_string(notification, &["notificationType", "notification_type"])
        .or_else(|| pick_string(data, &["notificationType", "type"]))
        .unwrap_or_else(|| "UNKNOWN".to_string());
    let subtype = pick_string(notification, &["subtype"]);
    let source_event_type = subtype
        .clone()
        .map(|subtype| format!("{event_type_raw}.{subtype}"))
        .unwrap_or_else(|| event_type_raw.clone());
    let product_id = pick_string(transaction, &["productId", "product_id"]);
    let bundle_id = pick_string(data, &["bundleId", "bundle_id"])
        .or_else(|| pick_string(transaction, &["bundleId", "bundle_id"]));
    let source_product_key = product_id
        .as_deref()
        .map(|id| source_product_key("app_store", bundle_id.as_deref(), id, None));
    let product_kind = pick_string(transaction, &["type"])
        .map(|value| match value.as_str() {
            "Auto-Renewable Subscription" | "auto_renewable_subscription" => {
                "subscription".to_string()
            }
            "Consumable" | "consumable" => "consumable".to_string(),
            "Non-Consumable" | "non_consumable" => "non_consumable".to_string(),
            "Non-Renewing Subscription" | "non_renewing_subscription" => "subscription".to_string(),
            _ => infer_product_kind(&event_type_raw, product_id.as_deref().unwrap_or("")),
        })
        .unwrap_or_else(|| {
            infer_product_kind(&event_type_raw, product_id.as_deref().unwrap_or(""))
        });
    let environment = event_environment(notification)
        .or_else(|| event_environment(data))
        .or_else(|| event_environment(transaction))
        .unwrap_or_else(|| "unknown".to_string());
    let mut warnings = vec![];
    if payload.get("signedPayload").is_some() {
        warnings.push(
            "App Store signedPayload and nested JWS payloads were decoded for ingestion."
                .to_string(),
        );
    }
    if payload.get("signedPayload").is_some() && decoded_signed_payload.is_none() {
        warnings.push("App Store signedPayload could not be decoded as compact JWS.".to_string());
    }
    if data.get("signedTransactionInfo").is_some() && decoded_transaction.is_none() {
        warnings.push(
            "App Store signedTransactionInfo could not be decoded as compact JWS.".to_string(),
        );
    }
    if environment == "unknown" {
        warnings.push("App Store payload did not include an environment field.".to_string());
    }

    ExtractedEvent {
        source_event_id: pick_string(
            notification,
            &["notificationUUID", "notification_uuid", "id"],
        )
        .or_else(|| pick_string(transaction, &["transactionId", "transaction_id"]))
        .unwrap_or_else(|| fallback_id.to_string()),
        source_event_type,
        environment,
        source_app_id: bundle_id,
        source_product_key,
        external_product_id: product_id.clone(),
        external_base_plan_id: None,
        external_offer_id: pick_string(transaction, &["offerIdentifier", "offer_id"]),
        display_name: product_id.clone(),
        product_kind,
        billing_period: infer_billing_period(product_id.as_deref().unwrap_or("")),
        platform: Some("ios".to_string()),
        normalized_event_type: map_app_store_type(&event_type_raw, subtype.as_deref()),
        amount_minor: apple_price_minor(transaction)
            .or_else(|| pick_money(transaction, &["amount_minor", "amount"])),
        currency: pick_string(transaction, &["currency"]),
        country: pick_string(transaction, &["country", "storefront", "storefrontId"]),
        customer_key: pick_string(transaction, &["appAccountToken", "app_account_token"]),
        transaction_key: pick_string(transaction, &["transactionId", "transaction_id"]),
        original_transaction_key: pick_string(
            transaction,
            &["originalTransactionId", "original_transaction_id"],
        ),
        subscription_key: pick_string(
            transaction,
            &["originalTransactionId", "original_transaction_id"],
        ),
        occurred_at: pick_optional_time(notification, &["signedDate"])
            .or_else(|| {
                pick_optional_time(transaction, &["signedDate", "purchaseDate", "expiresDate"])
            })
            .unwrap_or_else(OffsetDateTime::now_utc),
        purchase_time: pick_optional_time(transaction, &["purchaseDate", "originalPurchaseDate"]),
        period_start: pick_optional_time(transaction, &["purchaseDate"]),
        period_end: pick_optional_time(transaction, &["expiresDate", "revocationDate"]),
        will_renew: pick_string(
            decoded_renewal.as_ref().unwrap_or(data),
            &["autoRenewStatus"],
        )
        .and_then(|value| match value.as_str() {
            "1" | "true" => Some(true),
            "0" | "false" => Some(false),
            _ => None,
        }),
        warnings,
    }
}

fn extract_google_play(payload: &Value, fallback_id: &str) -> ExtractedEvent {
    let decoded = decode_pubsub_data(payload).unwrap_or_else(|| payload.clone());
    let source = decoded.as_object().map(|_| &decoded).unwrap_or(payload);
    let subscription = source.get("subscriptionNotification");
    let one_time = source.get("oneTimeProductNotification");
    let test_notification = source.get("testNotification");
    let event = subscription.or(one_time).unwrap_or(source);
    let event_type_raw = pick_string(event, &["notificationType", "event_type", "type"])
        .unwrap_or_else(|| "UNKNOWN".to_string());
    let lookup = source.get("googlePlayPurchaseLookup");
    let product_id = pick_string(event, &["subscriptionId", "sku", "productId", "product_id"])
        .or_else(|| lookup.and_then(|value| pick_string(value, &["product_id"])));
    let package_name = pick_string(source, &["packageName", "package_name"]);
    let base_plan_id = pick_string(event, &["basePlanId", "base_plan_id"])
        .or_else(|| lookup.and_then(|value| pick_string(value, &["base_plan_id"])));
    let environment = lookup
        .and_then(event_environment)
        .or_else(|| {
            if test_notification.is_some() {
                Some("test".to_string())
            } else {
                event_environment(source)
            }
        })
        .unwrap_or_else(|| "unknown".to_string());
    let source_product_key = product_id.as_deref().map(|id| {
        source_product_key(
            "google_play",
            package_name.as_deref(),
            id,
            base_plan_id.as_deref(),
        )
    });
    let amount_minor = pick_money(source, &["amount_minor", "price"])
        .or_else(|| lookup.and_then(|value| pick_money(value, &["amount_minor"])));
    let currency = pick_string(source, &["currency"])
        .or_else(|| lookup.and_then(|value| pick_string(value, &["currency"])));
    let mut warnings = vec![];
    if amount_minor.is_none() {
        warnings.push(
            "Google Play RTDN does not include price by default; revenue may be zero unless the webhook payload includes amount fields."
                .to_string(),
        );
    }
    if subscription.is_some() && product_id.is_none() {
        warnings.push(
            "Google Play subscription notification did not include a product id.".to_string(),
        );
    }
    if environment == "unknown" && (subscription.is_some() || one_time.is_some()) {
        warnings.push(
            "Google Play RTDN alone does not identify whether this purchase is production or test; configure Android Publisher API access to verify purchase tokens."
                .to_string(),
        );
    }
    if let Some(error) = lookup.and_then(|value| pick_string(value, &["error"])) {
        warnings.push(format!(
            "Google Play purchase environment lookup failed: {error}"
        ));
    }

    ExtractedEvent {
        source_event_id: pick_string(
            payload.get("message").unwrap_or(payload),
            &["messageId", "message_id", "id"],
        )
        .or_else(|| pick_string(source, &["pubsubMessageId", "messageId", "message_id"]))
        .or_else(|| pick_string(event, &["purchaseToken", "purchase_token"]))
        .unwrap_or_else(|| fallback_id.to_string()),
        source_event_type: event_type_raw.clone(),
        environment,
        source_app_id: package_name,
        source_product_key,
        external_product_id: product_id.clone(),
        external_base_plan_id: base_plan_id,
        external_offer_id: pick_string(event, &["offerId", "offer_id"]),
        display_name: product_id.clone(),
        product_kind: if subscription.is_some() {
            "subscription".to_string()
        } else {
            infer_product_kind(&event_type_raw, product_id.as_deref().unwrap_or(""))
        },
        billing_period: infer_billing_period(product_id.as_deref().unwrap_or("")),
        platform: Some("android".to_string()),
        normalized_event_type: map_google_type(
            &event_type_raw,
            subscription.is_some(),
            one_time.is_some(),
        ),
        amount_minor,
        currency,
        country: pick_string(source, &["country", "regionCode"])
            .or_else(|| lookup.and_then(|value| pick_string(value, &["region_code"]))),
        customer_key: pick_string(source, &["obfuscatedExternalAccountId", "customer_id"]).or_else(
            || lookup.and_then(|value| pick_string(value, &["obfuscated_external_account_id"])),
        ),
        transaction_key: lookup
            .and_then(|value| pick_string(value, &["order_id"]))
            .or_else(|| pick_string(event, &["orderId", "order_id"]))
            .or_else(|| pick_string(event, &["purchaseToken", "purchase_token"])),
        original_transaction_key: pick_string(event, &["purchaseToken", "purchase_token"]),
        subscription_key: pick_string(event, &["purchaseToken", "purchase_token"]),
        occurred_at: pick_time(
            source,
            &["eventTimeMillis", "event_time_millis", "occurred_at"],
        ),
        purchase_time: lookup
            .and_then(|value| pick_optional_time(value, &["purchase_time", "start_time"])),
        period_start: lookup.and_then(|value| pick_optional_time(value, &["start_time"])),
        period_end: lookup.and_then(|value| pick_optional_time(value, &["expiry_time"])),
        will_renew: lookup.and_then(|value| pick_bool(value, &["will_renew"])),
        warnings,
    }
}

fn extract_stripe(payload: &Value, fallback_id: &str) -> ExtractedEvent {
    let data = payload.pointer("/data/object").unwrap_or(payload);
    let event_type = pick_string(payload, &["type"]).unwrap_or_else(|| "stripe.event".to_string());
    let price = data
        .pointer("/lines/data/0/price")
        .or_else(|| data.get("price"))
        .unwrap_or(data);
    let product_id = pick_string(price, &["id", "price", "price_id"])
        .or_else(|| pick_string(data, &["price_id", "product_id"]));
    let source_product_key = product_id.as_deref().map(|id| {
        source_product_key(
            "stripe",
            pick_string(payload, &["account"]).as_deref(),
            id,
            None,
        )
    });
    ExtractedEvent {
        source_event_id: pick_string(payload, &["id"]).unwrap_or_else(|| fallback_id.to_string()),
        source_event_type: event_type.clone(),
        environment: event_environment(payload)
            .or_else(|| event_environment(data))
            .unwrap_or_else(|| "unknown".to_string()),
        source_app_id: pick_string(payload, &["account"]),
        source_product_key,
        external_product_id: product_id.clone(),
        external_base_plan_id: None,
        external_offer_id: None,
        display_name: product_id.clone(),
        product_kind: if event_type.contains("subscription") {
            "subscription"
        } else {
            "non_consumable"
        }
        .to_string(),
        billing_period: pick_string(price, &["recurring.interval"])
            .unwrap_or_else(|| infer_billing_period(product_id.as_deref().unwrap_or(""))),
        platform: Some("web".to_string()),
        normalized_event_type: Some(map_common_type(&event_type)),
        amount_minor: pick_money(data, &["amount_paid", "amount", "unit_amount"]),
        currency: pick_string(data, &["currency"]).map(|c| c.to_ascii_uppercase()),
        country: pick_string(data, &["country"]),
        customer_key: pick_string(data, &["customer", "customer_id"]),
        transaction_key: pick_string(data, &["id", "payment_intent", "charge"]),
        original_transaction_key: None,
        subscription_key: pick_string(data, &["subscription"]),
        occurred_at: pick_time(payload, &["created"]).max(pick_time(data, &["created"])),
        purchase_time: pick_optional_time(data, &["created", "created_at"]),
        period_start: pick_optional_time(data, &["period_start", "current_period_start"]),
        period_end: pick_optional_time(data, &["period_end", "current_period_end"]),
        will_renew: pick_bool(data, &["will_renew"])
            .or_else(|| pick_bool(data, &["cancel_at_period_end"]).map(|value| !value)),
        warnings: vec![
            "Stripe connector is webhook-only in this MVP; metrics use the received event payload."
                .to_string(),
        ],
    }
}

fn extract_paddle(payload: &Value, fallback_id: &str) -> ExtractedEvent {
    let data = payload.get("data").unwrap_or(payload);
    let event_type =
        pick_string(payload, &["event_type", "type"]).unwrap_or_else(|| "paddle.event".to_string());
    let product_id = pick_string(data, &["price_id", "product_id", "id"]);
    let source_product_key = product_id
        .as_deref()
        .map(|id| source_product_key("paddle", None, id, None));
    ExtractedEvent {
        source_event_id: pick_string(payload, &["event_id", "id"])
            .unwrap_or_else(|| fallback_id.to_string()),
        source_event_type: event_type.clone(),
        environment: event_environment(payload)
            .or_else(|| event_environment(data))
            .unwrap_or_else(|| "unknown".to_string()),
        source_app_id: None,
        source_product_key,
        external_product_id: product_id.clone(),
        external_base_plan_id: None,
        external_offer_id: None,
        display_name: product_id.clone(),
        product_kind: if event_type.contains("subscription") {
            "subscription"
        } else {
            "non_consumable"
        }
        .to_string(),
        billing_period: infer_billing_period(product_id.as_deref().unwrap_or("")),
        platform: Some("web".to_string()),
        normalized_event_type: Some(map_common_type(&event_type)),
        amount_minor: pick_money(data, &["amount", "amount_minor", "total"]),
        currency: pick_string(data, &["currency_code", "currency"]).map(|c| c.to_ascii_uppercase()),
        country: pick_string(data, &["country_code", "country"]),
        customer_key: pick_string(data, &["customer_id", "customer"]),
        transaction_key: pick_string(data, &["transaction_id", "id"]),
        original_transaction_key: None,
        subscription_key: pick_string(data, &["subscription_id", "subscription"]),
        occurred_at: pick_time(payload, &["occurred_at", "event_time"])
            .max(pick_time(data, &["created_at"])),
        purchase_time: pick_optional_time(data, &["created_at", "billed_at"]),
        period_start: pick_optional_time(
            data,
            &["current_billing_period.starts_at", "period_start"],
        ),
        period_end: pick_optional_time(data, &["current_billing_period.ends_at", "period_end"]),
        will_renew: pick_bool(data, &["will_renew", "scheduled_change"]),
        warnings: vec![
            "Paddle connector is webhook-only in this MVP; metrics use the received event payload."
                .to_string(),
        ],
    }
}

fn extract_generic(source_type: &str, payload: &Value, fallback_id: &str) -> ExtractedEvent {
    let mut event = extract_custom(payload, fallback_id);
    if let Some(product_id) = event.external_product_id.as_deref() {
        event.source_product_key = Some(source_product_key(
            source_type,
            event.source_app_id.as_deref(),
            product_id,
            event.external_base_plan_id.as_deref(),
        ));
    }
    event
}

fn pick_string(value: &Value, names: &[&str]) -> Option<String> {
    for name in names {
        if let Some(found) = value
            .get(*name)
            .or_else(|| value.pointer(&format!("/{}", name.replace('.', "/"))))
        {
            match found {
                Value::String(text) if !text.trim().is_empty() => {
                    return Some(text.trim().to_string());
                }
                Value::Number(number) => return Some(number.to_string()),
                Value::Bool(flag) => return Some(flag.to_string()),
                _ => {}
            }
        }
    }
    None
}

fn pick_money(value: &Value, names: &[&str]) -> Option<i64> {
    for name in names {
        if let Some(found) = value
            .get(*name)
            .or_else(|| value.pointer(&format!("/{}", name.replace('.', "/"))))
        {
            if name.ends_with("_minor")
                && let Some(number) = found.as_i64()
            {
                return Some(number);
            }
            if let Some(amount) = normalize_money_minor(found) {
                return Some(amount);
            }
        }
    }
    None
}

fn pick_time(value: &Value, names: &[&str]) -> OffsetDateTime {
    pick_optional_time(value, names).unwrap_or_else(OffsetDateTime::now_utc)
}

fn pick_optional_time(value: &Value, names: &[&str]) -> Option<OffsetDateTime> {
    for name in names {
        if let Some(found) = value
            .get(*name)
            .or_else(|| value.pointer(&format!("/{}", name.replace('.', "/"))))
            && let Some(time) = parse_time(found)
        {
            return Some(time);
        }
    }
    None
}

fn pick_bool(value: &Value, names: &[&str]) -> Option<bool> {
    for name in names {
        if let Some(found) = value
            .get(*name)
            .or_else(|| value.pointer(&format!("/{}", name.replace('.', "/"))))
        {
            match found {
                Value::Bool(value) => return Some(*value),
                Value::Number(value) => return value.as_i64().map(|value| value != 0),
                Value::String(value) => match value.trim().to_ascii_lowercase().as_str() {
                    "true" | "1" | "yes" => return Some(true),
                    "false" | "0" | "no" => return Some(false),
                    _ => {}
                },
                _ => {}
            }
        }
    }
    None
}

fn event_environment(value: &Value) -> Option<String> {
    for name in [
        "environment",
        "env",
        "store_environment",
        "source_environment",
    ] {
        if let Some(found) = value
            .get(name)
            .or_else(|| value.pointer(&format!("/{}", name.replace('.', "/"))))
        {
            match found {
                Value::String(text) => {
                    if let Some(environment) = normalize_environment(text) {
                        return Some(environment);
                    }
                }
                Value::Bool(flag) => {
                    return Some(if *flag { "production" } else { "test" }.to_string());
                }
                _ => {}
            }
        }
    }

    if let Some(flag) = value.get("livemode").and_then(Value::as_bool) {
        return Some(if flag { "production" } else { "test" }.to_string());
    }
    if matches!(value.get("sandbox").and_then(Value::as_bool), Some(true)) {
        return Some("sandbox".to_string());
    }

    None
}

fn normalize_environment(raw: &str) -> Option<String> {
    let normalized = raw.trim().to_ascii_lowercase().replace([' ', '-'], "_");
    if normalized.is_empty() {
        return None;
    }
    Some(
        match normalized.as_str() {
            "prod" | "production" | "live" | "real" => "production",
            "sandbox" => "sandbox",
            "test" | "testing" | "license_test" | "test_purchase" => "test",
            "unknown" => "unknown",
            other => other,
        }
        .to_string(),
    )
}

fn decode_pubsub_data(payload: &Value) -> Option<Value> {
    let encoded = payload.pointer("/message/data")?.as_str()?;
    let bytes = STANDARD.decode(encoded).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn decode_jws_payload(jws: &str) -> Option<Value> {
    let payload = jws.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn apple_price_minor(value: &Value) -> Option<i64> {
    let price = value.get("price")?;
    let raw = match price {
        Value::Number(number) => number.as_i64()?,
        Value::String(text) => text.parse::<i64>().ok()?,
        _ => return None,
    };
    // App Store transaction price is expressed in milliunits.
    Some(raw / 10)
}

fn normalize_platform(raw: String) -> String {
    let lower = raw.to_ascii_lowercase();
    if lower.contains("app_store") || lower.contains("ios") || lower.contains("mac") {
        "ios".to_string()
    } else if lower.contains("play") || lower.contains("android") {
        "android".to_string()
    } else {
        lower
    }
}

fn map_revenuecat_type(raw: &str) -> Option<String> {
    let upper = raw.to_ascii_uppercase();
    Some(
        match upper.as_str() {
            "INITIAL_PURCHASE" | "NON_RENEWING_PURCHASE" => "purchase",
            "RENEWAL" => "renewal",
            "CANCELLATION" => "cancellation",
            "EXPIRATION" => "expiration",
            "UNCANCELLATION" => "reactivation",
            "BILLING_ISSUE" => "billing_issue",
            "PRODUCT_CHANGE" => "product_change",
            "TRANSFER" => "product_change",
            "REFUND" => "refund",
            "TRIAL_STARTED" => "trial_started",
            "TRIAL_CONVERTED" => "trial_converted",
            _ => return Some(map_common_type(raw)),
        }
        .to_string(),
    )
}

fn map_app_store_type(raw: &str, subtype: Option<&str>) -> Option<String> {
    let upper = raw.to_ascii_uppercase();
    let subtype = subtype.unwrap_or_default().to_ascii_uppercase();
    Some(
        match upper.as_str() {
            "SUBSCRIBED" if subtype == "RESUBSCRIBE" => "reactivation",
            "SUBSCRIBED" => "purchase",
            "DID_RECOVER" => "reactivation",
            "DID_RENEW" => "renewal",
            "DID_CHANGE_RENEWAL_STATUS" if subtype == "AUTO_RENEW_ENABLED" => "reactivation",
            "DID_CHANGE_RENEWAL_STATUS" if subtype == "AUTO_RENEW_DISABLED" => "cancellation",
            "DID_CHANGE_RENEWAL_STATUS" => "unknown",
            "DID_CHANGE_RENEWAL_PREF" => "product_change",
            "EXPIRED" => "expiration",
            "REFUND" => "refund",
            "REFUND_DECLINED" => "refund_declined",
            "CONSUMPTION_REQUEST" => "consumption",
            "GRACE_PERIOD_EXPIRED" => "expiration",
            "DID_FAIL_TO_RENEW" if subtype == "GRACE_PERIOD" => "grace_period_started",
            "DID_FAIL_TO_RENEW" => "billing_issue",
            "REVOKE" => "revocation",
            _ => return Some(map_common_type(raw)),
        }
        .to_string(),
    )
}

fn map_google_type(raw: &str, is_subscription: bool, is_one_time: bool) -> Option<String> {
    let normalized = raw.to_ascii_lowercase();
    if is_one_time {
        return Some(
            match normalized.as_str() {
                "1" | "one_time_product_purchased" => "one_time_purchase",
                "2" | "one_time_product_canceled" => "cancellation",
                _ => "unknown",
            }
            .to_string(),
        );
    }
    if !is_subscription {
        return Some("unknown".to_string());
    }
    Some(
        match normalized.as_str() {
            "1" | "subscription_recovered" => "reactivation",
            "2" | "subscription_renewed" => "renewal",
            "3" | "subscription_canceled" => "cancellation",
            "4" | "subscription_purchased" => "purchase",
            "5" | "subscription_on_hold" => "billing_issue",
            "6" | "subscription_in_grace_period" => "grace_period_started",
            "7" | "subscription_restarted" => "reactivation",
            "12" | "subscription_revoked" => "revocation",
            "13" | "subscription_expired" => "expiration",
            _ => return Some(map_common_type(raw)),
        }
        .to_string(),
    )
}

fn map_common_type(raw: &str) -> String {
    let lower = raw.to_ascii_lowercase();
    if lower.contains("trial") && lower.contains("convert") {
        "trial_converted"
    } else if lower.contains("trial") {
        "trial_started"
    } else if lower.contains("renew") || lower.contains("invoice.paid") {
        "renewal"
    } else if lower.contains("cancel") {
        "cancellation"
    } else if lower.contains("expire") {
        "expiration"
    } else if lower.contains("refund") || lower.contains("charge.refunded") {
        "refund"
    } else if lower.contains("revoke") || lower.contains("chargeback") {
        "revocation"
    } else if lower.contains("billing") || lower.contains("fail") {
        "billing_issue"
    } else if lower.contains("reactivat") || lower.contains("recover") {
        "reactivation"
    } else if lower.contains("consume") {
        "consumption"
    } else if lower.contains("purchase") || lower.contains("paid") || lower.contains("payment") {
        "purchase"
    } else {
        "unknown"
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn google_subscription_renewal_uses_order_id_for_each_charge() {
        let payload = json!({
            "packageName": "com.example.app",
            "eventTimeMillis": "1710000000000",
            "subscriptionNotification": {
                "notificationType": 2,
                "purchaseToken": "shared-purchase-token",
                "subscriptionId": "pro.monthly"
            },
            "googlePlayPurchaseLookup": {
                "status": "verified",
                "environment": "production",
                "order_id": "GPA.1234-5678-9012-34567..2",
                "start_time": "2024-03-01T00:00:00Z",
                "expiry_time": "2024-04-01T00:00:00Z",
                "will_renew": true,
                "amount_minor": 999,
                "currency": "USD"
            }
        });

        let event = extract_event("google_play", &payload, "fallback");
        assert_eq!(event.normalized_event_type.as_deref(), Some("renewal"));
        assert_eq!(
            event.transaction_key.as_deref(),
            Some("GPA.1234-5678-9012-34567..2")
        );
        assert_eq!(
            event.subscription_key.as_deref(),
            Some("shared-purchase-token")
        );
        assert_eq!(event.amount_minor, Some(999));
        assert_eq!(event.environment, "production");
        assert_eq!(event.will_renew, Some(true));
        assert!(event.period_end.is_some());
    }

    #[test]
    fn app_store_preserves_purchase_time_separately_from_notification_time() {
        let payload = json!({
            "notificationType": "DID_RENEW",
            "notificationUUID": "notification-1",
            "signedDate": 1710003600000_i64,
            "data": {
                "bundleId": "com.example.app",
                "environment": "Production",
                "transactionInfo": {
                    "transactionId": "2000000000001",
                    "originalTransactionId": "1000000000001",
                    "productId": "pro.monthly",
                    "type": "Auto-Renewable Subscription",
                    "purchaseDate": 1710000000000_i64,
                    "expiresDate": 1712678400000_i64,
                    "price": 9990,
                    "currency": "USD"
                },
                "signedRenewalInfo": null
            }
        });

        let event = extract_event("app_store", &payload, "fallback");
        assert_eq!(event.normalized_event_type.as_deref(), Some("renewal"));
        assert_eq!(event.amount_minor, Some(999));
        assert_eq!(event.environment, "production");
        assert_eq!(event.transaction_key.as_deref(), Some("2000000000001"));
        assert!(event.purchase_time.is_some());
        assert!(event.period_end.is_some());
        assert!(event.occurred_at > event.purchase_time.unwrap());
    }

    #[test]
    fn non_financial_store_notifications_do_not_create_revenue() {
        assert_eq!(
            map_app_store_type("REFUND_DECLINED", None).as_deref(),
            Some("refund_declined")
        );
        assert_eq!(
            map_google_type("1", false, true).as_deref(),
            Some("one_time_purchase")
        );
        assert_eq!(map_common_type("UNRECOGNIZED_PROVIDER_EVENT"), "unknown");
    }
}
