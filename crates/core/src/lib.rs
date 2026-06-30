use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use time::{Date, OffsetDateTime};
use uuid::Uuid;

pub const EVENT_TYPES: &[&str] = &[
    "purchase",
    "one_time_purchase",
    "trial_started",
    "trial_converted",
    "renewal",
    "cancellation",
    "expiration",
    "refund",
    "partial_refund",
    "revocation",
    "billing_issue",
    "grace_period_started",
    "grace_period_ended",
    "reactivation",
    "product_change",
    "consumption",
];

pub const PRODUCT_KINDS: &[&str] = &[
    "subscription",
    "consumable",
    "non_consumable",
    "lifetime",
    "unknown",
];

pub const BILLING_PERIODS: &[&str] =
    &["weekly", "monthly", "annual", "lifetime", "none", "unknown"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricValue<T> {
    pub value: T,
    pub definition: String,
    pub estimated: bool,
    pub trust_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Period {
    pub from: Date,
    pub to: Date,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverviewMetrics {
    pub period: Period,
    pub currency: String,
    pub metrics: OverviewMetricSet,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverviewMetricSet {
    pub gross_revenue_minor: MetricValue<i64>,
    pub net_revenue_minor: MetricValue<i64>,
    pub refund_amount_minor: MetricValue<i64>,
    pub active_subscriptions: MetricValue<i64>,
    pub new_subscriptions: MetricValue<i64>,
    pub renewals: MetricValue<i64>,
    pub churned_subscriptions: MetricValue<i64>,
    pub refund_rate: MetricValue<f64>,
}

pub fn new_id(prefix: &str) -> String {
    format!("{prefix}_{}", Uuid::now_v7().as_simple())
}

pub fn payload_sha256(payload: &Value) -> String {
    let bytes = serde_json::to_vec(payload).unwrap_or_default();
    let digest = Sha256::digest(bytes);
    hex_lower(&digest)
}

pub fn sha256_hex(input: impl AsRef<[u8]>) -> String {
    let digest = Sha256::digest(input.as_ref());
    hex_lower(&digest)
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

pub fn today_utc() -> Date {
    OffsetDateTime::now_utc().date()
}

pub fn infer_billing_period(raw: &str) -> String {
    let lower = raw.to_ascii_lowercase();
    if lower.contains("week") || lower.contains("weekly") || lower.contains("_wk") {
        "weekly".to_string()
    } else if lower.contains("month") || lower.contains("monthly") || lower.contains("_mo") {
        "monthly".to_string()
    } else if lower.contains("year")
        || lower.contains("annual")
        || lower.contains("annually")
        || lower.contains("_yr")
    {
        "annual".to_string()
    } else if lower.contains("life") || lower.contains("lifetime") {
        "lifetime".to_string()
    } else {
        "unknown".to_string()
    }
}

pub fn infer_product_kind(event_type: &str, product_id: &str) -> String {
    let lower_product = product_id.to_ascii_lowercase();
    let lower_event = event_type.to_ascii_lowercase();
    if lower_product.contains("life") || lower_product.contains("lifetime") {
        "lifetime".to_string()
    } else if lower_event.contains("renew")
        || lower_event.contains("subscription")
        || lower_product.contains("month")
        || lower_product.contains("annual")
        || lower_product.contains("year")
        || lower_product.contains("week")
    {
        "subscription".to_string()
    } else if lower_product.contains("coin")
        || lower_product.contains("credit")
        || lower_product.contains("token")
        || lower_product.contains("pack")
    {
        "consumable".to_string()
    } else if lower_product.is_empty() {
        "unknown".to_string()
    } else {
        "non_consumable".to_string()
    }
}

pub fn normalize_money_minor(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => {
            if let Some(raw) = number.as_i64() {
                Some(raw)
            } else {
                number.as_f64().map(|float| (float * 100.0).round() as i64)
            }
        }
        Value::String(text) => text
            .parse::<f64>()
            .ok()
            .map(|float| (float * 100.0).round() as i64),
        _ => None,
    }
}

pub fn parse_time(value: &Value) -> Option<OffsetDateTime> {
    match value {
        Value::String(text) => {
            OffsetDateTime::parse(text, &time::format_description::well_known::Rfc3339)
                .ok()
                .or_else(|| parse_millis(text.parse::<i128>().ok()?))
        }
        Value::Number(number) => {
            if let Some(raw) = number.as_i64() {
                parse_millis(raw as i128)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn parse_millis(raw: i128) -> Option<OffsetDateTime> {
    let seconds = if raw > 10_000_000_000 {
        raw / 1000
    } else {
        raw
    };
    OffsetDateTime::from_unix_timestamp(seconds as i64).ok()
}

pub fn source_product_key(
    source_type: &str,
    source_app_id: Option<&str>,
    product_id: &str,
    base_plan_id: Option<&str>,
) -> String {
    match source_type {
        "google_play" => format!(
            "google_play:{}:{}:{}",
            source_app_id.unwrap_or("unknown"),
            product_id,
            base_plan_id.unwrap_or("")
        ),
        "app_store" => format!(
            "app_store:{}:{product_id}",
            source_app_id.unwrap_or("unknown")
        ),
        "revenuecat" => format!(
            "revenuecat:{}:{product_id}",
            source_app_id.unwrap_or("unknown")
        ),
        "stripe" => format!("stripe:{}:{product_id}", source_app_id.unwrap_or("unknown")),
        "paddle" => format!("paddle:{}:{product_id}", source_app_id.unwrap_or("unknown")),
        "csv" => format!("csv:{}:{product_id}", source_app_id.unwrap_or("unknown")),
        "custom_api" => format!(
            "custom_api:{}:{product_id}",
            source_app_id.unwrap_or("unknown")
        ),
        other => format!(
            "{other}:{}:{product_id}",
            source_app_id.unwrap_or("unknown")
        ),
    }
}

pub fn is_revenue_event(event_type: &str) -> bool {
    matches!(
        event_type,
        "purchase" | "one_time_purchase" | "trial_converted" | "renewal"
    )
}

pub fn is_refund_event(event_type: &str) -> bool {
    matches!(event_type, "refund" | "partial_refund" | "revocation")
}
