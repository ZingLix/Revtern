use anyhow::{Context, Result};
use revtern_connectors::{ExtractedEvent, extract_event};
use revtern_core::{is_refund_event, is_revenue_event, new_id};
use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use time::OffsetDateTime;

struct ProjectionContext<'a> {
    workspace_id: &'a str,
    app_id: Option<&'a str>,
    source_product_id: Option<&'a str>,
    logical_product_id: Option<&'a str>,
    source_type: &'a str,
    extracted: &'a ExtractedEvent,
}

pub async fn enqueue_normalization(pool: &PgPool, raw_event_id: &str) -> Result<String> {
    let id = new_id("job");
    sqlx::query(
        r#"
        insert into jobs (id, queue, job_type, payload, status, run_after, attempts, max_attempts, created_at)
        values ($1, 'default', 'normalize_raw_event', $2, 'queued', now(), 0, 5, now())
        "#,
    )
    .bind(&id)
    .bind(json!({ "raw_event_id": raw_event_id }))
    .execute(pool)
    .await?;
    Ok(id)
}

pub async fn process_normalization_job(pool: &PgPool, job_id: &str, worker_id: &str) -> Result<()> {
    let row = sqlx::query(
        r#"
        update jobs
        set status = 'running', locked_at = now(), locked_by = $2, attempts = attempts + 1
        where id = $1
          and (status in ('queued', 'failed') or (status = 'running' and locked_at < now() - interval '5 minutes'))
        returning payload
        "#,
    )
    .bind(job_id)
    .bind(worker_id)
    .fetch_one(pool)
    .await?;
    let payload: Value = row.try_get("payload")?;
    let raw_event_id = payload
        .get("raw_event_id")
        .and_then(Value::as_str)
        .context("job payload missing raw_event_id")?;

    match process_raw_event(pool, raw_event_id).await {
        Ok(()) => {
            sqlx::query(
                "update jobs set status = 'completed', locked_at = null, locked_by = null, last_error = null where id = $1",
            )
            .bind(job_id)
            .execute(pool)
            .await?;
            Ok(())
        }
        Err(error) => {
            let error_text = error.to_string();
            sqlx::query(
                r#"
                update jobs
                set status = case when attempts >= max_attempts then 'dead' else 'failed' end,
                    locked_at = null,
                    locked_by = null,
                    last_error = $2,
                    run_after = now() + (least(attempts, 6) * interval '30 seconds')
                where id = $1
                "#,
            )
            .bind(job_id)
            .bind(error_text)
            .execute(pool)
            .await?;
            Err(error)
        }
    }
}

pub async fn process_next_job(pool: &PgPool, worker_id: &str) -> Result<bool> {
    let job_id: Option<String> = sqlx::query_scalar(
        r#"
        select id
        from jobs
        where run_after <= now()
          and (status in ('queued', 'failed') or (status = 'running' and locked_at < now() - interval '5 minutes'))
        order by run_after asc, created_at asc
        limit 1
        "#,
    )
    .fetch_optional(pool)
    .await?;
    let Some(job_id) = job_id else {
        return Ok(false);
    };
    match process_normalization_job(pool, &job_id, worker_id).await {
        Ok(()) => Ok(true),
        Err(error)
            if matches!(
                error.downcast_ref::<sqlx::Error>(),
                Some(sqlx::Error::RowNotFound)
            ) =>
        {
            Ok(false)
        }
        Err(error) => Err(error),
    }
}

pub async fn process_raw_event(pool: &PgPool, raw_event_id: &str) -> Result<()> {
    let raw = sqlx::query(
        r#"
        select re.*, ds.app_id as source_app_id_hint
        from raw_events re
        join data_sources ds on ds.id = re.data_source_id
        where re.id = $1
        "#,
    )
    .bind(raw_event_id)
    .fetch_one(pool)
    .await?;

    let workspace_id: String = raw.try_get("workspace_id")?;
    let data_source_id: String = raw.try_get("data_source_id")?;
    let source_type: String = raw.try_get("source_type")?;
    let source_payload: Value = raw.try_get("payload")?;
    let processing_payload: Option<Value> = raw.try_get("processing_payload")?;
    let payload = processing_payload.unwrap_or(source_payload);
    let fallback_event_id: String = raw.try_get("source_event_id")?;
    let app_hint: Option<String> = raw.try_get("source_app_id_hint")?;
    let extracted = extract_event(&source_type, &payload, &fallback_event_id);
    let app_id = resolve_app_id(pool, &workspace_id, app_hint, &extracted).await?;
    let source_product_id = upsert_source_product(
        pool,
        &workspace_id,
        &data_source_id,
        app_id.as_deref(),
        &source_type,
        &extracted,
        &payload,
    )
    .await?;
    let logical_product_id = if let Some(source_product_id) = source_product_id.as_deref() {
        active_logical_product(pool, &workspace_id, source_product_id).await?
    } else {
        None
    };

    if let Some(event_type) = extracted.normalized_event_type.as_deref() {
        let projection = ProjectionContext {
            workspace_id: &workspace_id,
            app_id: app_id.as_deref(),
            source_product_id: source_product_id.as_deref(),
            logical_product_id: logical_product_id.as_deref(),
            source_type: &source_type,
            extracted: &extracted,
        };
        let (normalized_event_id, normalized_event_inserted) =
            insert_normalized_event(pool, raw_event_id, &data_source_id, event_type, &projection)
                .await?;
        let transaction_id =
            project_transaction(pool, &normalized_event_id, event_type, &projection).await?;
        project_subscription(pool, transaction_id.as_deref(), event_type, &projection).await?;
        if normalized_event_inserted {
            update_daily_metric(
                pool,
                &workspace_id,
                app_id.as_deref(),
                logical_product_id.as_deref(),
                &source_type,
                event_type,
                &extracted,
            )
            .await?;
        }
    }

    let status = if extracted.normalized_event_type.is_some() {
        "processed"
    } else {
        "stored"
    };
    sqlx::query(
        r#"
        update raw_events
        set source_event_type = $2,
            environment = $3,
            source_app_id = coalesce($4, source_app_id),
            source_product_id = coalesce($5, source_product_id),
            occurred_at = $6,
            processing_status = $7,
            processing_error = null
        where id = $1
        "#,
    )
    .bind(raw_event_id)
    .bind(&extracted.source_event_type)
    .bind(&extracted.environment)
    .bind(&extracted.source_app_id)
    .bind(source_product_id.as_deref())
    .bind(extracted.occurred_at)
    .bind(status)
    .execute(pool)
    .await?;

    Ok(())
}

async fn resolve_app_id(
    pool: &PgPool,
    workspace_id: &str,
    app_hint: Option<String>,
    extracted: &ExtractedEvent,
) -> Result<Option<String>> {
    if app_hint.is_some() {
        return Ok(app_hint);
    }

    if let Some(source_app_id) = extracted.source_app_id.as_deref()
        && let Some(row) = sqlx::query(
            r#"
            select id from apps
            where workspace_id = $1
              and ($2 = apple_bundle_id or $2 = google_package_name or $2 = platform_bundle_id)
            limit 1
            "#,
        )
        .bind(workspace_id)
        .bind(source_app_id)
        .fetch_optional(pool)
        .await?
    {
        let id: String = row.try_get("id")?;
        return Ok(Some(id));
    }

    let row =
        sqlx::query("select id from apps where workspace_id = $1 order by created_at asc limit 1")
            .bind(workspace_id)
            .fetch_optional(pool)
            .await?;
    row.map(|row| row.try_get("id"))
        .transpose()
        .map_err(Into::into)
}

async fn upsert_source_product(
    pool: &PgPool,
    workspace_id: &str,
    data_source_id: &str,
    app_id: Option<&str>,
    source_type: &str,
    extracted: &ExtractedEvent,
    payload: &Value,
) -> Result<Option<String>> {
    let Some(source_product_key) = extracted.source_product_key.as_deref() else {
        return Ok(None);
    };
    let id = new_id("sp");
    let row = sqlx::query(
        r#"
        insert into source_products (
          id, workspace_id, data_source_id, app_id, source_type, platform,
          external_product_id, external_base_plan_id, external_offer_id, external_price_id,
          display_name, product_kind, billing_period, amount_minor, currency,
          raw_metadata, mapping_state, source_product_key, first_seen_at, last_seen_at
        )
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, null, $10, $11, $12, $13, $14, $15, 'unmapped', $16, now(), now())
        on conflict (workspace_id, data_source_id, source_product_key)
        do update set
          last_seen_at = now(),
          display_name = coalesce(excluded.display_name, source_products.display_name),
          product_kind = case when source_products.product_kind = 'unknown' then excluded.product_kind else source_products.product_kind end,
          billing_period = case when source_products.billing_period = 'unknown' then excluded.billing_period else source_products.billing_period end,
          amount_minor = coalesce(source_products.amount_minor, excluded.amount_minor),
          currency = coalesce(source_products.currency, excluded.currency),
          app_id = coalesce(source_products.app_id, excluded.app_id),
          raw_metadata = source_products.raw_metadata || excluded.raw_metadata
        returning id
        "#,
    )
    .bind(&id)
    .bind(workspace_id)
    .bind(data_source_id)
    .bind(app_id)
    .bind(source_type)
    .bind(&extracted.platform)
    .bind(&extracted.external_product_id)
    .bind(&extracted.external_base_plan_id)
    .bind(&extracted.external_offer_id)
    .bind(&extracted.display_name)
    .bind(&extracted.product_kind)
    .bind(&extracted.billing_period)
    .bind(extracted.amount_minor)
    .bind(&extracted.currency)
    .bind(json!({
        "first_payload": payload,
        "warnings": extracted.warnings,
    }))
    .bind(source_product_key)
    .fetch_one(pool)
    .await?;
    let source_product_id: String = row.try_get("id")?;
    Ok(Some(source_product_id))
}

async fn active_logical_product(
    pool: &PgPool,
    workspace_id: &str,
    source_product_id: &str,
) -> Result<Option<String>> {
    let row = sqlx::query(
        r#"
        select logical_product_id
        from product_mappings
        where workspace_id = $1 and source_product_id = $2 and active = true
        order by created_at desc
        limit 1
        "#,
    )
    .bind(workspace_id)
    .bind(source_product_id)
    .fetch_optional(pool)
    .await?;
    row.map(|row| row.try_get("logical_product_id"))
        .transpose()
        .map_err(Into::into)
}

async fn insert_normalized_event(
    pool: &PgPool,
    raw_event_id: &str,
    data_source_id: &str,
    event_type: &str,
    projection: &ProjectionContext<'_>,
) -> Result<(String, bool)> {
    let ProjectionContext {
        workspace_id,
        app_id,
        source_product_id,
        logical_product_id,
        extracted,
        ..
    } = projection;
    let id = new_id("ne");
    let warnings = json!(extracted.warnings);
    let inserted = sqlx::query(
        r#"
        insert into normalized_events (
          id, workspace_id, raw_event_id, data_source_id, app_id, source_product_id, logical_product_id,
          event_type, platform, customer_key, transaction_key, original_transaction_key, subscription_key,
          amount_minor, currency, country, occurred_at, environment, normalization_version, confidence, warnings, created_at
        )
        values (
          $1, $2, $3, $4, $5, $6, $7,
          $8, $9, $10, $11, $12, $13,
          $14, $15, $16, $17, $18, 'mvp_v1', $19, $20, now()
        )
        on conflict (raw_event_id, event_type) do nothing
        returning id
        "#,
    )
    .bind(&id)
    .bind(*workspace_id)
    .bind(raw_event_id)
    .bind(data_source_id)
    .bind(*app_id)
    .bind(*source_product_id)
    .bind(*logical_product_id)
    .bind(event_type)
    .bind(&extracted.platform)
    .bind(&extracted.customer_key)
    .bind(&extracted.transaction_key)
    .bind(&extracted.original_transaction_key)
    .bind(&extracted.subscription_key)
    .bind(extracted.amount_minor)
    .bind(&extracted.currency)
    .bind(&extracted.country)
    .bind(extracted.occurred_at)
    .bind(&extracted.environment)
    .bind(if extracted.warnings.is_empty() { 0.92_f64 } else { 0.68_f64 })
    .bind(&warnings)
    .fetch_optional(pool)
    .await?;

    if let Some(row) = inserted {
        return Ok((row.try_get("id")?, true));
    }

    sqlx::query(
        r#"
        update normalized_events
        set app_id = coalesce(app_id, $3),
            source_product_id = coalesce(source_product_id, $4),
            logical_product_id = coalesce(logical_product_id, $5),
            environment = $6,
            warnings = $7
        where raw_event_id = $1 and event_type = $2
        "#,
    )
    .bind(raw_event_id)
    .bind(event_type)
    .bind(*app_id)
    .bind(*source_product_id)
    .bind(*logical_product_id)
    .bind(&extracted.environment)
    .bind(&warnings)
    .execute(pool)
    .await?;

    // Return the existing id too when an event was already normalized.
    let row = sqlx::query(
        "select id from normalized_events where raw_event_id = $1 and event_type = $2 order by created_at desc limit 1",
    )
    .bind(raw_event_id)
    .bind(event_type)
    .fetch_one(pool)
    .await?;
    Ok((row.try_get("id")?, false))
}

async fn project_transaction(
    pool: &PgPool,
    normalized_event_id: &str,
    event_type: &str,
    projection: &ProjectionContext<'_>,
) -> Result<Option<String>> {
    let ProjectionContext {
        workspace_id,
        app_id,
        source_product_id,
        logical_product_id,
        source_type,
        extracted,
    } = projection;
    if !is_revenue_event(event_type) && !is_refund_event(event_type) {
        return Ok(None);
    }

    let transaction_key = extracted
        .transaction_key
        .clone()
        .unwrap_or_else(|| normalized_event_id.to_string());
    let status = match event_type {
        "renewal" => "renewed",
        "refund" => "refunded",
        "partial_refund" => "partially_refunded",
        "revocation" => "revoked",
        _ => "paid",
    };
    let amount = extracted.amount_minor.unwrap_or(0);
    let currency = extracted.currency.as_deref().unwrap_or("UNKNOWN");
    let row = sqlx::query(
        r#"
        insert into transactions (
          id, workspace_id, app_id, source_product_id, logical_product_id, customer_id,
          platform, transaction_key, original_transaction_key, source_type, environment, purchase_time,
          amount_minor, currency, country, status, source_status, status_reason, status_updated_at,
          refunded_at, refund_amount_minor, created_from_event_id, latest_event_id, updated_at
        )
        values (
          $1, $2, $3, $4, $5, null,
          $6, $7, $8, $9, $10, $11,
          $12, $13, $14, $15, $16, null, $20,
          $17, $18, $19, $19, now()
        )
        on conflict (workspace_id, source_type, transaction_key)
        do update set
          environment = excluded.environment,
          purchase_time = least(transactions.purchase_time, excluded.purchase_time),
          amount_minor = case when transactions.amount_minor = 0 then excluded.amount_minor else transactions.amount_minor end,
          currency = case when transactions.currency = 'UNKNOWN' then excluded.currency else transactions.currency end,
          country = coalesce(transactions.country, excluded.country),
          status = case when excluded.status_updated_at >= transactions.status_updated_at then excluded.status else transactions.status end,
          source_status = case when excluded.status_updated_at >= transactions.status_updated_at then excluded.source_status else transactions.source_status end,
          status_updated_at = greatest(transactions.status_updated_at, excluded.status_updated_at),
          refunded_at = coalesce(excluded.refunded_at, transactions.refunded_at),
          refund_amount_minor = case
            when excluded.refund_amount_minor is null then transactions.refund_amount_minor
            when transactions.latest_event_id = excluded.latest_event_id then transactions.refund_amount_minor
            else coalesce(transactions.refund_amount_minor, 0) + excluded.refund_amount_minor
          end,
          latest_event_id = case when excluded.status_updated_at >= transactions.status_updated_at then excluded.latest_event_id else transactions.latest_event_id end,
          logical_product_id = coalesce(transactions.logical_product_id, excluded.logical_product_id),
          updated_at = now()
        returning id
        "#,
    )
    .bind(new_id("txn"))
    .bind(*workspace_id)
    .bind(*app_id)
    .bind(*source_product_id)
    .bind(*logical_product_id)
    .bind(&extracted.platform)
    .bind(&transaction_key)
    .bind(&extracted.original_transaction_key)
    .bind(*source_type)
    .bind(&extracted.environment)
    .bind(extracted.purchase_time.unwrap_or(extracted.occurred_at))
    .bind(amount)
    .bind(currency)
    .bind(&extracted.country)
    .bind(status)
    .bind(&extracted.source_event_type)
    .bind(if is_refund_event(event_type) { Some(extracted.occurred_at) } else { None })
    .bind(if is_refund_event(event_type) { Some(amount.abs()) } else { None })
    .bind(normalized_event_id)
    .bind(extracted.occurred_at)
    .fetch_one(pool)
    .await?;
    let transaction_id: String = row.try_get("id")?;

    if let Some(customer_key) = extracted.customer_key.as_deref() {
        upsert_customer(
            pool,
            workspace_id,
            customer_key,
            source_type,
            extracted.occurred_at,
        )
        .await?;
        sqlx::query(
            r#"
            update transactions
            set customer_id = (
              select id from customers
              where workspace_id = $1
                and coalesce(app_user_id, revenuecat_app_user_id, google_obfuscated_account_id, apple_app_account_token) = $2
              limit 1
            )
            where id = $3
            "#,
        )
        .bind(*workspace_id)
        .bind(customer_key)
        .bind(&transaction_id)
        .execute(pool)
        .await?;
    }

    Ok(Some(transaction_id))
}

async fn project_subscription(
    pool: &PgPool,
    latest_transaction_id: Option<&str>,
    event_type: &str,
    projection: &ProjectionContext<'_>,
) -> Result<()> {
    let ProjectionContext {
        workspace_id,
        app_id,
        source_product_id,
        logical_product_id,
        source_type,
        extracted,
    } = projection;
    if extracted.product_kind != "subscription"
        && !matches!(
            event_type,
            "trial_started"
                | "renewal"
                | "cancellation"
                | "expiration"
                | "billing_issue"
                | "grace_period_started"
                | "grace_period_ended"
                | "reactivation"
        )
    {
        return Ok(());
    }
    let Some(subscription_key) = extracted
        .subscription_key
        .clone()
        .or_else(|| extracted.original_transaction_key.clone())
        .or_else(|| extracted.transaction_key.clone())
    else {
        return Ok(());
    };
    let status = match event_type {
        "trial_started" => "trialing",
        "cancellation" => "cancelled_active",
        "expiration" => "expired",
        "billing_issue" => "billing_retry",
        "grace_period_started" => "grace_period",
        "grace_period_ended" | "reactivation" => "active",
        "refund" | "revocation" => "refunded",
        _ => "active",
    };
    sqlx::query(
        r#"
        insert into subscriptions (
          id, workspace_id, app_id, source_product_id, logical_product_id, customer_id,
          platform, subscription_key, original_transaction_key, environment, status, started_at,
          current_period_start, current_period_end, cancelled_at, expired_at, will_renew,
          in_grace_period, in_billing_retry, latest_transaction_id, status_updated_at, updated_at
        )
        values (
          $1, $2, $3, $4, $5, null,
          $6, $7, $8, $9, $10, $11,
          $12, $13, $14, $15, $16,
          $17, $18, $19, $20, now()
        )
        on conflict (workspace_id, subscription_key)
        do update set
          environment = excluded.environment,
          status = case when excluded.status_updated_at >= subscriptions.status_updated_at then excluded.status else subscriptions.status end,
          status_updated_at = greatest(subscriptions.status_updated_at, excluded.status_updated_at),
          source_product_id = coalesce(subscriptions.source_product_id, excluded.source_product_id),
          logical_product_id = coalesce(subscriptions.logical_product_id, excluded.logical_product_id),
          started_at = least(subscriptions.started_at, excluded.started_at),
          current_period_start = case when excluded.status_updated_at >= subscriptions.status_updated_at then coalesce(excluded.current_period_start, subscriptions.current_period_start) else subscriptions.current_period_start end,
          current_period_end = case when excluded.status_updated_at >= subscriptions.status_updated_at then coalesce(excluded.current_period_end, subscriptions.current_period_end) else subscriptions.current_period_end end,
          cancelled_at = coalesce(excluded.cancelled_at, subscriptions.cancelled_at),
          expired_at = coalesce(excluded.expired_at, subscriptions.expired_at),
          will_renew = case when excluded.status_updated_at >= subscriptions.status_updated_at then excluded.will_renew else subscriptions.will_renew end,
          in_grace_period = case when excluded.status_updated_at >= subscriptions.status_updated_at then excluded.in_grace_period else subscriptions.in_grace_period end,
          in_billing_retry = case when excluded.status_updated_at >= subscriptions.status_updated_at then excluded.in_billing_retry else subscriptions.in_billing_retry end,
          latest_transaction_id = case when excluded.status_updated_at >= subscriptions.status_updated_at then coalesce(excluded.latest_transaction_id, subscriptions.latest_transaction_id) else subscriptions.latest_transaction_id end,
          updated_at = now()
        "#,
    )
    .bind(new_id("sub"))
    .bind(*workspace_id)
    .bind(*app_id)
    .bind(*source_product_id)
    .bind(*logical_product_id)
    .bind(&extracted.platform)
    .bind(&subscription_key)
    .bind(&extracted.original_transaction_key)
    .bind(&extracted.environment)
    .bind(status)
    .bind(extracted.purchase_time.unwrap_or(extracted.occurred_at))
    .bind(extracted.period_start.or(extracted.purchase_time))
    .bind(extracted.period_end)
    .bind(if event_type == "cancellation" { Some(extracted.occurred_at) } else { None })
    .bind(if event_type == "expiration" { Some(extracted.occurred_at) } else { None })
    .bind(extracted.will_renew.unwrap_or(!matches!(event_type, "cancellation" | "expiration")))
    .bind(event_type == "grace_period_started")
    .bind(event_type == "billing_issue")
    .bind(latest_transaction_id)
    .bind(extracted.occurred_at)
    .execute(pool)
    .await?;

    if let Some(customer_key) = extracted.customer_key.as_deref() {
        upsert_customer(
            pool,
            workspace_id,
            customer_key,
            source_type,
            extracted.occurred_at,
        )
        .await?;
        sqlx::query(
            r#"
            update subscriptions
            set customer_id = (
              select id from customers
              where workspace_id = $1 and customer_identity_key = $2
              limit 1
            )
            where workspace_id = $1 and subscription_key = $3
            "#,
        )
        .bind(*workspace_id)
        .bind(customer_key)
        .bind(&subscription_key)
        .execute(pool)
        .await?;
    }
    Ok(())
}

async fn update_daily_metric(
    pool: &PgPool,
    workspace_id: &str,
    app_id: Option<&str>,
    logical_product_id: Option<&str>,
    source_type: &str,
    event_type: &str,
    extracted: &ExtractedEvent,
) -> Result<()> {
    if extracted.environment != "production" {
        return Ok(());
    }

    let currency = extracted.currency.as_deref().unwrap_or("UNKNOWN");
    let gross = if is_revenue_event(event_type) {
        extracted.amount_minor.unwrap_or(0)
    } else {
        0
    };
    let refund = if is_refund_event(event_type) {
        extracted.amount_minor.unwrap_or(0).abs()
    } else {
        0
    };
    let purchase_count = i64::from(matches!(
        event_type,
        "purchase" | "one_time_purchase" | "trial_converted"
    ));
    let renewal_count = i64::from(event_type == "renewal");
    let new_subscription_count =
        i64::from(event_type == "purchase" && extracted.product_kind == "subscription");
    let cancel_count = i64::from(event_type == "cancellation");
    let expiration_count = i64::from(event_type == "expiration");
    let refund_count = i64::from(is_refund_event(event_type));
    let trial_start_count = i64::from(event_type == "trial_started");
    let trial_conversion_count = i64::from(event_type == "trial_converted");

    sqlx::query(
        r#"
        insert into daily_metrics (
          id, workspace_id, date, app_id, platform, logical_product_id, country, currency, source_type,
          gross_revenue_minor, estimated_proceeds_minor, refund_amount_minor, net_revenue_minor,
          purchase_count, renewal_count, new_subscription_count, active_subscription_count,
          cancel_count, expiration_count, refund_count, trial_start_count, trial_conversion_count
        )
        values (
          $1, $2, $3, $4, $5, $6, $7, $8, $9,
          $10, 0, $11, $12,
          $13, $14, $15, 0,
          $16, $17, $18, $19, $20
        )
        on conflict (workspace_id, date, app_id_key, platform_key, logical_product_id_key, country_key, currency, source_type)
        do update set
          gross_revenue_minor = daily_metrics.gross_revenue_minor + excluded.gross_revenue_minor,
          refund_amount_minor = daily_metrics.refund_amount_minor + excluded.refund_amount_minor,
          net_revenue_minor = daily_metrics.net_revenue_minor + excluded.net_revenue_minor,
          purchase_count = daily_metrics.purchase_count + excluded.purchase_count,
          renewal_count = daily_metrics.renewal_count + excluded.renewal_count,
          new_subscription_count = daily_metrics.new_subscription_count + excluded.new_subscription_count,
          cancel_count = daily_metrics.cancel_count + excluded.cancel_count,
          expiration_count = daily_metrics.expiration_count + excluded.expiration_count,
          refund_count = daily_metrics.refund_count + excluded.refund_count,
          trial_start_count = daily_metrics.trial_start_count + excluded.trial_start_count,
          trial_conversion_count = daily_metrics.trial_conversion_count + excluded.trial_conversion_count
        "#,
    )
    .bind(new_id("dm"))
    .bind(workspace_id)
    .bind(extracted.occurred_at.date())
    .bind(app_id)
    .bind(&extracted.platform)
    .bind(logical_product_id)
    .bind(&extracted.country)
    .bind(currency)
    .bind(source_type)
    .bind(gross)
    .bind(refund)
    .bind(gross - refund)
    .bind(purchase_count)
    .bind(renewal_count)
    .bind(new_subscription_count)
    .bind(cancel_count)
    .bind(expiration_count)
    .bind(refund_count)
    .bind(trial_start_count)
    .bind(trial_conversion_count)
    .execute(pool)
    .await?;
    Ok(())
}

async fn upsert_customer(
    pool: &PgPool,
    workspace_id: &str,
    customer_key: &str,
    source_type: &str,
    occurred_at: OffsetDateTime,
) -> Result<()> {
    let (app_user_id, apple, google, revenuecat) = match source_type {
        "app_store" => (None, Some(customer_key), None, None),
        "google_play" => (None, None, Some(customer_key), None),
        "revenuecat" => (None, None, None, Some(customer_key)),
        _ => (Some(customer_key), None, None, None),
    };
    sqlx::query(
        r#"
        insert into customers (
          id, workspace_id, app_user_id, apple_app_account_token, google_obfuscated_account_id,
          revenuecat_app_user_id, first_seen_at, last_seen_at
        )
        values ($1, $2, $3, $4, $5, $6, $7, $7)
        on conflict (workspace_id, customer_identity_key)
        do update set last_seen_at = excluded.last_seen_at
        "#,
    )
    .bind(new_id("cus"))
    .bind(workspace_id)
    .bind(app_user_id)
    .bind(apple)
    .bind(google)
    .bind(revenuecat)
    .bind(occurred_at)
    .execute(pool)
    .await?;
    Ok(())
}
