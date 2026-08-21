use anyhow::{Context, Result, bail};
use axum::http::HeaderMap;
use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use openssl::{
    bn::BigNum,
    ecdsa::EcdsaSig,
    hash::MessageDigest,
    memcmp,
    sign::Verifier,
    stack::Stack,
    x509::{X509, X509StoreContext, store::X509StoreBuilder},
};
use serde::Deserialize;
use serde_json::Value;

const APPLE_ROOT_CERTIFICATES_PEM: &str = include_str!("../certs/apple-root-certificates.pem");

#[derive(Debug, Deserialize)]
struct AppleJwsHeader {
    alg: String,
    x5c: Vec<String>,
}

pub fn verify_shared_secret(
    secret_hash: Option<&str>,
    headers: &HeaderMap,
    payload: &Value,
) -> bool {
    let Some(secret_hash) = secret_hash else {
        return false;
    };
    let header_secret = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            headers
                .get("x-revtern-webhook-secret")
                .and_then(|value| value.to_str().ok())
        })
        .or_else(|| {
            headers
                .get("x-revenuecat-authorization")
                .and_then(|value| value.to_str().ok())
        });
    let candidate = header_secret.or_else(|| payload.get("shared_secret").and_then(Value::as_str));
    candidate.is_some_and(|candidate| {
        let candidate_hash = revtern_core::sha256_hex(candidate.as_bytes());
        candidate_hash.len() == secret_hash.len()
            && memcmp::eq(candidate_hash.as_bytes(), secret_hash.as_bytes())
    })
}

pub fn verify_app_store_payload(payload: &Value, credentials: Option<&Value>) -> Result<Value> {
    let credentials =
        credentials.context("App Store verification credentials are required (bundle_id)")?;
    let signed_payload = payload
        .get("signedPayload")
        .and_then(Value::as_str)
        .context("App Store Server Notifications V2 payload must contain signedPayload")?;
    let roots = apple_root_certificates(credentials)?;
    let decoded = verify_apple_jws(signed_payload, &roots)
        .context("App Store signedPayload verification failed")?;

    let data = decoded
        .get("data")
        .context("verified App Store payload is missing data")?;
    let expected_bundle = credential_string(credentials, "bundle_id")
        .context("App Store verification requires bundle_id")?;
    let actual_bundle = data
        .get("bundleId")
        .and_then(Value::as_str)
        .context("verified App Store payload is missing data.bundleId")?;
    if actual_bundle != expected_bundle {
        bail!("App Store bundle id does not match the configured app");
    }

    let actual_environment = data
        .get("environment")
        .and_then(Value::as_str)
        .map(normalize_apple_environment)
        .context("verified App Store payload is missing data.environment")?;
    let expected_environment = credential_string(credentials, "environment")
        .map(normalize_apple_environment)
        .unwrap_or("both");
    if expected_environment != "both" && expected_environment != actual_environment {
        bail!("App Store environment does not match the configured source");
    }

    if actual_environment == "production" {
        let expected_app_id = credential_string(credentials, "app_apple_id")
            .context("production App Store verification requires app_apple_id")?;
        let actual_app_id = data
            .get("appAppleId")
            .and_then(value_as_string)
            .context("production App Store payload is missing data.appAppleId")?;
        if actual_app_id != expected_app_id {
            bail!("App Store appAppleId does not match the configured app");
        }
    }

    for field in ["signedTransactionInfo", "signedRenewalInfo"] {
        if let Some(jws) = data.get(field).and_then(Value::as_str) {
            verify_apple_jws(jws, &roots)
                .with_context(|| format!("App Store {field} verification failed"))?;
        }
    }

    Ok(decoded)
}

pub async fn verify_google_pubsub_oidc(
    headers: &HeaderMap,
    credentials: Option<&Value>,
) -> Result<bool> {
    let Some(credentials) = credentials else {
        return Ok(false);
    };
    let Some(expected_audience) = credential_string(credentials, "pubsub_oidc_audience") else {
        return Ok(false);
    };
    let expected_email = credential_string(credentials, "pubsub_service_account_email")
        .context("Google Pub/Sub OIDC verification requires pubsub_service_account_email")?;
    let token = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("Google Pub/Sub push is missing its Bearer token")?;

    let response = reqwest::Client::new()
        .get("https://oauth2.googleapis.com/tokeninfo")
        .query(&[("id_token", token)])
        .send()
        .await
        .context("Google Pub/Sub token verification request failed")?;
    if !response.status().is_success() {
        bail!("Google Pub/Sub Bearer token is invalid");
    }
    let claims: Value = response
        .json()
        .await
        .context("invalid Google token verification response")?;
    let audience_matches = claims.get("aud").is_some_and(|aud| match aud {
        Value::String(value) => value == expected_audience,
        Value::Array(values) => values
            .iter()
            .any(|value| value.as_str() == Some(expected_audience)),
        _ => false,
    });
    let issuer_matches = matches!(
        claims.get("iss").and_then(Value::as_str),
        Some("accounts.google.com" | "https://accounts.google.com")
    );
    let email_matches = claims.get("email").and_then(Value::as_str) == Some(expected_email);
    let email_verified = claims
        .get("email_verified")
        .is_some_and(|value| value.as_bool() == Some(true) || value.as_str() == Some("true"));
    if !audience_matches || !issuer_matches || !email_matches || !email_verified {
        bail!(
            "Google Pub/Sub token claims do not match the configured audience and service account"
        );
    }
    Ok(true)
}

pub fn verify_google_play_package(payload: &Value, credentials: Option<&Value>) -> Result<()> {
    let Some(expected_package) =
        credentials.and_then(|value| credential_string(value, "package_name"))
    else {
        return Ok(());
    };
    let notification = payload
        .get("message")
        .and_then(|message| message.get("data"))
        .and_then(Value::as_str)
        .map(|encoded| {
            let bytes = STANDARD
                .decode(encoded)
                .context("invalid Google Pub/Sub message.data encoding")?;
            serde_json::from_slice::<Value>(&bytes)
                .context("invalid Google Play developer notification JSON")
        })
        .transpose()?
        .unwrap_or_else(|| payload.clone());
    let actual_package = notification
        .get("packageName")
        .or_else(|| notification.get("package_name"))
        .and_then(Value::as_str)
        .context("Google Play developer notification is missing packageName")?;
    if actual_package != expected_package {
        bail!("Google Play package name does not match the configured app");
    }
    Ok(())
}

fn verify_apple_jws(jws: &str, roots: &[X509]) -> Result<Value> {
    let mut parts = jws.split('.');
    let header_part = parts.next().context("JWS header is missing")?;
    let payload_part = parts.next().context("JWS payload is missing")?;
    let signature_part = parts.next().context("JWS signature is missing")?;
    if parts.next().is_some() {
        bail!("JWS must have exactly three components");
    }
    let header: AppleJwsHeader = serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(header_part)
            .context("invalid JWS header encoding")?,
    )?;
    if header.alg != "ES256" || header.x5c.is_empty() {
        bail!("App Store JWS must use ES256 and include an x5c certificate chain");
    }

    let certificates = header
        .x5c
        .iter()
        .map(|encoded| {
            let der = STANDARD
                .decode(encoded)
                .context("invalid x5c certificate encoding")?;
            X509::from_der(&der).context("invalid x5c certificate")
        })
        .collect::<Result<Vec<_>>>()?;
    let leaf = certificates
        .first()
        .context("JWS leaf certificate is missing")?;
    let mut store_builder = X509StoreBuilder::new()?;
    for root in roots {
        store_builder.add_cert(root.clone())?;
    }
    let store = store_builder.build();
    let mut untrusted = Stack::new()?;
    for certificate in certificates.iter().skip(1) {
        untrusted.push(certificate.clone())?;
    }
    let mut context = X509StoreContext::new()?;
    let verified = context.init(&store, leaf, &untrusted, |context| context.verify_cert())?;
    if !verified {
        bail!("App Store x5c certificate chain is not trusted");
    }

    let signature = URL_SAFE_NO_PAD
        .decode(signature_part)
        .context("invalid JWS signature encoding")?;
    if signature.len() != 64 {
        bail!("App Store ES256 signature must be 64 bytes");
    }
    let ecdsa_signature = EcdsaSig::from_private_components(
        BigNum::from_slice(&signature[..32])?,
        BigNum::from_slice(&signature[32..])?,
    )?
    .to_der()?;
    let public_key = leaf.public_key()?;
    let mut verifier = Verifier::new(MessageDigest::sha256(), &public_key)?;
    verifier.update(format!("{header_part}.{payload_part}").as_bytes())?;
    if !verifier.verify(&ecdsa_signature)? {
        bail!("App Store JWS signature is invalid");
    }

    let payload = URL_SAFE_NO_PAD
        .decode(payload_part)
        .context("invalid JWS payload encoding")?;
    serde_json::from_slice(&payload).context("invalid JWS JSON payload")
}

fn apple_root_certificates(credentials: &Value) -> Result<Vec<X509>> {
    let mut roots = X509::stack_from_pem(APPLE_ROOT_CERTIFICATES_PEM.as_bytes())
        .context("parse bundled Apple root certificates")?;
    if let Some(values) = credentials
        .get("apple_root_certificates")
        .and_then(Value::as_array)
    {
        for value in values {
            let pem = value
                .as_str()
                .context("apple_root_certificates entries must be PEM strings")?;
            roots.extend(
                X509::stack_from_pem(pem.as_bytes())
                    .context("invalid Apple root certificate PEM")?,
            );
        }
    }
    if let Some(pem) = credential_string(credentials, "apple_root_ca_pem") {
        roots.extend(
            X509::stack_from_pem(pem.as_bytes()).context("invalid Apple root certificate PEM")?,
        );
    }
    Ok(roots)
}

fn credential_string<'a>(credentials: &'a Value, key: &str) -> Option<&'a str> {
    credentials
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn value_as_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

fn normalize_apple_environment(value: &str) -> &str {
    match value.trim().to_ascii_lowercase().as_str() {
        "production" | "prod" => "production",
        "sandbox" => "sandbox",
        "xcode" | "localtesting" | "test" => "test",
        "both" | "any" => "both",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderMap, HeaderValue};
    use serde_json::json;

    #[test]
    fn shared_secret_accepts_only_the_configured_value() {
        let secret_hash = revtern_core::sha256_hex("correct-secret");
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-revtern-webhook-secret",
            HeaderValue::from_static("correct-secret"),
        );
        assert!(verify_shared_secret(
            Some(&secret_hash),
            &headers,
            &json!({})
        ));

        headers.insert(
            "x-revtern-webhook-secret",
            HeaderValue::from_static("wrong-secret"),
        );
        assert!(!verify_shared_secret(
            Some(&secret_hash),
            &headers,
            &json!({})
        ));
    }

    #[test]
    fn app_store_verification_never_accepts_an_unsigned_body() {
        let credentials = json!({
            "bundle_id": "com.example.app",
            "environment": "sandbox"
        });
        assert!(verify_app_store_payload(&json!({}), Some(&credentials)).is_err());
    }

    #[test]
    fn bundled_apple_root_certificates_are_available() {
        let roots = apple_root_certificates(&json!({})).expect("bundled Apple roots");
        assert_eq!(roots.len(), 3);
    }

    #[test]
    fn google_play_package_must_match_the_configured_app() {
        let credentials = json!({ "package_name": "com.example.app" });
        let matching = json!({
            "message": {
                "data": STANDARD.encode(br#"{"packageName":"com.example.app"}"#)
            }
        });
        let mismatched = json!({
            "message": {
                "data": STANDARD.encode(br#"{"packageName":"com.example.other"}"#)
            }
        });

        assert!(verify_google_play_package(&matching, Some(&credentials)).is_ok());
        assert!(verify_google_play_package(&mismatched, Some(&credentials)).is_err());
    }
}
