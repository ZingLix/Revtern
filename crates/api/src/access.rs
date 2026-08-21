use serde::Serialize;
use sqlx::Row;

use crate::{
    auth::CurrentUser,
    error::{ApiError, ApiResult},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    AppRead,
    ExportRun,
    AppWrite,
    CatalogWrite,
    SourceWrite,
    SourceCredentialsWrite,
    JobsRun,
    MembersManage,
}

impl Capability {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AppRead => "app.read",
            Self::ExportRun => "export.run",
            Self::AppWrite => "app.write",
            Self::CatalogWrite => "catalog.write",
            Self::SourceWrite => "source.write",
            Self::SourceCredentialsWrite => "source.credentials.write",
            Self::JobsRun => "jobs.run",
            Self::MembersManage => "members.manage",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AppAccess {
    pub app_id: String,
    pub workspace_id: String,
    pub role: String,
    pub permissions: Vec<String>,
}

pub async fn app_access(
    pool: &sqlx::PgPool,
    user_id: &str,
    app_id: &str,
) -> ApiResult<Option<AppAccess>> {
    let row = sqlx::query(
        r#"
        select a.id as app_id,
               a.workspace_id,
               case
                 when a.owner_user_id = $1 then 'owner'
                 when wu.role in ('owner', 'admin') then 'workspace_admin'
                 else coalesce(ar.role_key, 'viewer')
               end as access_role,
               coalesce(array_agg(distinct eap.permission) filter (where eap.permission is not null), '{}') as permissions
        from apps a
        join effective_app_permissions eap on eap.app_id = a.id and eap.user_id = $1
        left join workspace_users wu
          on wu.workspace_id = a.workspace_id and wu.user_id = $1 and wu.status = 'active'
        left join app_memberships am on am.app_id = a.id and am.user_id = $1
        left join app_roles ar on ar.id = am.role_id
        where a.id = $2 and a.deleted_at is null
        group by a.id, a.workspace_id, a.owner_user_id, wu.role, ar.role_key
        "#,
    )
    .bind(user_id)
    .bind(app_id)
    .fetch_optional(pool)
    .await?;

    row.map(|row| {
        Ok(AppAccess {
            app_id: row.try_get("app_id")?,
            workspace_id: row.try_get("workspace_id")?,
            role: row.try_get("access_role")?,
            permissions: row.try_get("permissions")?,
        })
    })
    .transpose()
}

pub async fn require_app(
    pool: &sqlx::PgPool,
    user: &CurrentUser,
    app_id: &str,
    capability: Capability,
) -> ApiResult<AppAccess> {
    let access = app_access(pool, &user.user.id, app_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("app not found".to_string()))?;
    if !access
        .permissions
        .iter()
        .any(|permission| permission == capability.as_str())
    {
        return Err(ApiError::Forbidden(format!(
            "{} permission is required",
            capability.as_str()
        )));
    }
    Ok(access)
}

pub async fn audit(
    pool: &sqlx::PgPool,
    user: &CurrentUser,
    workspace_id: Option<&str>,
    app_id: Option<&str>,
    action: &str,
    target_type: Option<&str>,
    target_id: Option<&str>,
    metadata: serde_json::Value,
) -> ApiResult<()> {
    sqlx::query(
        r#"
        insert into audit_events (
          id, workspace_id, app_id, actor_user_id, action, target_type, target_id, metadata, created_at
        ) values ($1, $2, $3, $4, $5, $6, $7, $8, now())
        "#,
    )
    .bind(revtern_core::new_id("aud"))
    .bind(workspace_id)
    .bind(app_id)
    .bind(&user.user.id)
    .bind(action)
    .bind(target_type)
    .bind(target_id)
    .bind(metadata)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Capability;

    #[test]
    fn capability_names_are_stable() {
        assert_eq!(Capability::AppRead.as_str(), "app.read");
        assert_eq!(Capability::MembersManage.as_str(), "members.manage");
    }
}
