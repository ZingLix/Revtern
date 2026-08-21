alter table users
  alter column password_hash drop not null,
  add column if not exists status text not null default 'active',
  add column if not exists updated_at timestamptz not null default now();

alter table users drop constraint if exists users_status_check;
alter table users add constraint users_status_check
  check (status in ('active', 'suspended', 'disabled'));

create table if not exists auth_providers (
  id text primary key,
  provider_type text not null,
  name text not null,
  issuer text,
  client_id text,
  encrypted_client_secret text,
  scopes text not null default 'openid profile email',
  enabled boolean not null default true,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  unique (issuer, client_id)
);

create table if not exists auth_identities (
  id text primary key,
  user_id text not null references users(id) on delete cascade,
  provider_id text not null references auth_providers(id) on delete cascade,
  subject text not null,
  email text,
  email_verified boolean not null default false,
  claims jsonb not null default '{}'::jsonb,
  last_authenticated_at timestamptz,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  unique (provider_id, subject)
);

alter table sessions
  add column if not exists auth_identity_id text references auth_identities(id) on delete set null,
  add column if not exists auth_method text not null default 'local',
  add column if not exists revoked_at timestamptz,
  add column if not exists idle_expires_at timestamptz;

alter table workspace_users
  add column if not exists status text not null default 'active',
  add column if not exists invited_by_user_id text references users(id) on delete set null,
  add column if not exists created_at timestamptz not null default now(),
  add column if not exists updated_at timestamptz not null default now();

alter table workspace_users drop constraint if exists workspace_users_status_check;
alter table workspace_users add constraint workspace_users_status_check
  check (status in ('invited', 'active', 'suspended'));

alter table apps
  add column if not exists owner_user_id text references users(id) on delete restrict,
  add column if not exists created_by_user_id text references users(id) on delete set null,
  add column if not exists version bigint not null default 1,
  add column if not exists deleted_at timestamptz;

update apps a
set owner_user_id = coalesce(
      (
        select wu.user_id
        from workspace_users wu
        join users u on u.id = wu.user_id
        where wu.workspace_id = a.workspace_id and wu.role = 'owner'
        order by u.created_at asc
        limit 1
      ),
      (
        select wu.user_id
        from workspace_users wu
        join users u on u.id = wu.user_id
        where wu.workspace_id = a.workspace_id
        order by u.created_at asc
        limit 1
      )
    ),
    created_by_user_id = coalesce(
      created_by_user_id,
      (
        select wu.user_id
        from workspace_users wu
        join users u on u.id = wu.user_id
        where wu.workspace_id = a.workspace_id
        order by case when wu.role = 'owner' then 0 else 1 end, u.created_at asc
        limit 1
      )
    )
where owner_user_id is null or created_by_user_id is null;

do $$
begin
  if exists (select 1 from apps where owner_user_id is null) then
    raise exception 'cannot migrate apps without a workspace member to app ownership';
  end if;
end $$;

alter table apps alter column owner_user_id set not null;

create table if not exists app_roles (
  id text primary key,
  workspace_id text references workspaces(id) on delete cascade,
  role_key text not null,
  name text not null,
  description text not null default '',
  is_system boolean not null default false,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  unique (workspace_id, role_key)
);

create table if not exists app_role_permissions (
  role_id text not null references app_roles(id) on delete cascade,
  permission text not null,
  primary key (role_id, permission)
);

insert into app_roles (id, workspace_id, role_key, name, description, is_system)
values
  ('role_viewer', null, 'viewer', 'Viewer', 'Read dashboards, ledgers, products, and source health.', true),
  ('role_analyst', null, 'analyst', 'Analyst', 'Viewer access plus raw events and exports.', true),
  ('role_editor', null, 'editor', 'Editor', 'Analyst access plus app, catalog, and operational changes.', true),
  ('role_manager', null, 'manager', 'Manager', 'Editor access plus credentials and member management.', true)
on conflict (id) do nothing;

insert into app_role_permissions (role_id, permission)
values
  ('role_viewer', 'app.read'),
  ('role_viewer', 'ledger.read'),
  ('role_analyst', 'app.read'),
  ('role_analyst', 'ledger.read'),
  ('role_analyst', 'events.sensitive.read'),
  ('role_analyst', 'export.run'),
  ('role_editor', 'app.read'),
  ('role_editor', 'ledger.read'),
  ('role_editor', 'events.sensitive.read'),
  ('role_editor', 'export.run'),
  ('role_editor', 'app.write'),
  ('role_editor', 'catalog.write'),
  ('role_editor', 'source.write'),
  ('role_editor', 'jobs.run'),
  ('role_manager', 'app.read'),
  ('role_manager', 'ledger.read'),
  ('role_manager', 'events.sensitive.read'),
  ('role_manager', 'export.run'),
  ('role_manager', 'app.write'),
  ('role_manager', 'catalog.write'),
  ('role_manager', 'source.write'),
  ('role_manager', 'source.credentials.write'),
  ('role_manager', 'jobs.run'),
  ('role_manager', 'members.manage')
on conflict do nothing;

create table if not exists app_memberships (
  app_id text not null references apps(id) on delete cascade,
  user_id text not null references users(id) on delete cascade,
  role_id text not null references app_roles(id) on delete restrict,
  granted_by_user_id text references users(id) on delete set null,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  primary key (app_id, user_id)
);

-- Older reverse-proxy deployments added every discovered user as an owner of
-- the first workspace. Preserve their access to existing apps explicitly, but
-- move them to a personal workspace so they do not inherit future apps.
with ranked_workspace_admins as (
  select wu.workspace_id, wu.user_id,
         row_number() over (
           partition by wu.workspace_id
           order by case when wu.role = 'owner' then 0 else 1 end, u.created_at, wu.user_id
         ) as admin_rank
  from workspace_users wu
  join users u on u.id = wu.user_id
  where wu.role in ('owner', 'admin')
)
insert into workspaces (id, name, created_at)
select distinct
  'wsp_personal_' || substr(md5(r.user_id), 1, 20),
  coalesce(nullif(u.display_name, ''), u.email) || '''s Apps',
  now()
from ranked_workspace_admins r
join users u on u.id = r.user_id
where r.admin_rank > 1
on conflict (id) do nothing;

with ranked_workspace_admins as (
  select wu.workspace_id, wu.user_id,
         row_number() over (
           partition by wu.workspace_id
           order by case when wu.role = 'owner' then 0 else 1 end, u.created_at, wu.user_id
         ) as admin_rank
  from workspace_users wu
  join users u on u.id = wu.user_id
  where wu.role in ('owner', 'admin')
)
insert into workspace_users (workspace_id, user_id, role, status, created_at, updated_at)
select distinct 'wsp_personal_' || substr(md5(r.user_id), 1, 20), r.user_id,
       'owner', 'active', now(), now()
from ranked_workspace_admins r
where r.admin_rank > 1
on conflict (workspace_id, user_id) do nothing;

with ranked_workspace_admins as (
  select wu.workspace_id, wu.user_id,
         row_number() over (
           partition by wu.workspace_id
           order by case when wu.role = 'owner' then 0 else 1 end, u.created_at, wu.user_id
         ) as admin_rank
  from workspace_users wu
  join users u on u.id = wu.user_id
  where wu.role in ('owner', 'admin')
)
insert into app_memberships (app_id, user_id, role_id, created_at, updated_at)
select distinct a.id, r.user_id, 'role_manager', now(), now()
from ranked_workspace_admins r
join apps a on a.workspace_id = r.workspace_id and a.owner_user_id <> r.user_id
where r.admin_rank > 1
on conflict (app_id, user_id) do nothing;

with ranked_workspace_admins as (
  select wu.workspace_id, wu.user_id,
         row_number() over (
           partition by wu.workspace_id
           order by case when wu.role = 'owner' then 0 else 1 end, u.created_at, wu.user_id
         ) as admin_rank
  from workspace_users wu
  join users u on u.id = wu.user_id
  where wu.role in ('owner', 'admin')
)
update workspace_users wu
set role = 'guest', updated_at = now()
from ranked_workspace_admins r
where wu.workspace_id = r.workspace_id
  and wu.user_id = r.user_id
  and r.admin_rank > 1;

create table if not exists app_invitations (
  id text primary key,
  app_id text not null references apps(id) on delete cascade,
  email text not null,
  normalized_email text not null,
  role_id text not null references app_roles(id) on delete restrict,
  token_hash text not null unique,
  invited_by_user_id text references users(id) on delete set null,
  expires_at timestamptz not null,
  accepted_at timestamptz,
  accepted_by_user_id text references users(id) on delete set null,
  revoked_at timestamptz,
  created_at timestamptz not null default now()
);

create unique index if not exists app_invitations_pending_email_idx
on app_invitations (app_id, normalized_email)
where accepted_at is null and revoked_at is null;

create table if not exists audit_events (
  id text primary key,
  workspace_id text references workspaces(id) on delete set null,
  app_id text references apps(id) on delete set null,
  actor_user_id text references users(id) on delete set null,
  action text not null,
  target_type text,
  target_id text,
  request_id text,
  metadata jsonb not null default '{}'::jsonb,
  created_at timestamptz not null default now()
);

create index if not exists audit_events_app_created_idx
on audit_events (app_id, created_at desc);

create index if not exists auth_identities_user_idx on auth_identities (user_id, created_at);
create index if not exists sessions_user_active_idx on sessions (user_id, expires_at) where revoked_at is null;

create table if not exists oidc_transactions (
  state_hash text primary key,
  provider_id text not null references auth_providers(id) on delete cascade,
  nonce_hash text not null,
  encrypted_pkce_verifier text not null,
  return_to text not null default '/',
  link_user_id text references users(id) on delete cascade,
  invitation_token_hash text,
  expires_at timestamptz not null,
  created_at timestamptz not null default now()
);

create index if not exists oidc_transactions_expiry_idx on oidc_transactions (expires_at);

alter table sync_runs add column if not exists app_id text references apps(id) on delete cascade;

-- Every existing workspace with data that cannot be assigned to an app receives
-- an explicit quarantine app. This removes the unsafe "first app" fallback.
insert into apps (
  id, workspace_id, owner_user_id, created_by_user_id, name, default_currency,
  created_at, updated_at
)
select
  'app_legacy_' || substr(md5(w.id), 1, 20),
  w.id,
  owner.user_id,
  owner.user_id,
  'Legacy / Unassigned',
  'USD',
  now(),
  now()
from workspaces w
cross join lateral (
  select wu.user_id
  from workspace_users wu
  join users u on u.id = wu.user_id
  where wu.workspace_id = w.id
  order by case when wu.role = 'owner' then 0 else 1 end, u.created_at asc
  limit 1
) owner
where
  exists (select 1 from data_sources x where x.workspace_id = w.id and x.app_id is null)
  or exists (select 1 from logical_products x where x.workspace_id = w.id and x.app_id is null)
  or exists (select 1 from source_products x where x.workspace_id = w.id and x.app_id is null)
  or exists (select 1 from normalized_events x where x.workspace_id = w.id and x.app_id is null)
  or exists (select 1 from transactions x where x.workspace_id = w.id and x.app_id is null)
  or exists (select 1 from subscriptions x where x.workspace_id = w.id and x.app_id is null)
  or exists (select 1 from daily_metrics x where x.workspace_id = w.id and x.app_id is null)
  or exists (select 1 from sync_runs x where x.workspace_id = w.id and x.app_id is null)
  or exists (
    select 1
    from customers c
    where c.workspace_id = w.id
      and not exists (select 1 from transactions t where t.customer_id = c.id)
      and not exists (select 1 from subscriptions s where s.customer_id = c.id)
  )
on conflict (id) do nothing;

update data_sources ds
set app_id = 'app_legacy_' || substr(md5(ds.workspace_id), 1, 20)
where app_id is null;

update source_products sp
set app_id = coalesce(
  (select ds.app_id from data_sources ds where ds.id = sp.data_source_id),
  'app_legacy_' || substr(md5(sp.workspace_id), 1, 20)
)
where app_id is null;

update source_apps sa
set app_id = ds.app_id
from data_sources ds
where sa.data_source_id = ds.id and sa.app_id is distinct from ds.app_id;

update logical_products lp
set app_id = coalesce(
  (
    select sp.app_id
    from product_mappings pm
    join source_products sp on sp.id = pm.source_product_id
    where pm.logical_product_id = lp.id
    order by pm.created_at asc
    limit 1
  ),
  'app_legacy_' || substr(md5(lp.workspace_id), 1, 20)
)
where app_id is null;

alter table product_mappings add column if not exists app_id text references apps(id) on delete cascade;
update product_mappings pm
set app_id = sp.app_id
from source_products sp
where pm.source_product_id = sp.id and pm.app_id is null;

alter table raw_events add column if not exists app_id text references apps(id) on delete cascade;
update raw_events re
set app_id = ds.app_id
from data_sources ds
where re.data_source_id = ds.id and re.app_id is null;

update normalized_events ne
set app_id = coalesce(
  (select re.app_id from raw_events re where re.id = ne.raw_event_id),
  (select ds.app_id from data_sources ds where ds.id = ne.data_source_id),
  'app_legacy_' || substr(md5(ne.workspace_id), 1, 20)
)
where app_id is null;

update transactions t
set app_id = coalesce(
  (select ne.app_id from normalized_events ne where ne.id = t.created_from_event_id),
  (select sp.app_id from source_products sp where sp.id = t.source_product_id),
  'app_legacy_' || substr(md5(t.workspace_id), 1, 20)
)
where app_id is null;

update subscriptions s
set app_id = coalesce(
  (select t.app_id from transactions t where t.id = s.latest_transaction_id),
  (select sp.app_id from source_products sp where sp.id = s.source_product_id),
  'app_legacy_' || substr(md5(s.workspace_id), 1, 20)
)
where app_id is null;

update daily_metrics dm
set app_id = coalesce(
  (select lp.app_id from logical_products lp where lp.id = dm.logical_product_id),
  'app_legacy_' || substr(md5(dm.workspace_id), 1, 20)
)
where app_id is null;

update sync_runs sr
set app_id = coalesce(
  (select ds.app_id from data_sources ds where ds.id = sr.data_source_id),
  'app_legacy_' || substr(md5(sr.workspace_id), 1, 20)
)
where sr.app_id is null;

alter table jobs
  add column if not exists workspace_id text references workspaces(id) on delete cascade,
  add column if not exists app_id text references apps(id) on delete cascade,
  add column if not exists actor_user_id text references users(id) on delete set null;

update jobs j
set workspace_id = re.workspace_id,
    app_id = re.app_id
from raw_events re
where j.payload ->> 'raw_event_id' = re.id
  and (j.workspace_id is null or j.app_id is null);

alter table customers add column if not exists app_id text references apps(id) on delete cascade;
alter table customers drop constraint if exists customers_workspace_id_customer_identity_key_key;

create temporary table migration_customer_apps on commit drop as
select customer_id, app_id,
       row_number() over (partition by customer_id order by app_id) as app_rank
from (
  select customer_id, app_id from transactions where customer_id is not null
  union
  select customer_id, app_id from subscriptions where customer_id is not null
) scoped;

update customers c
set app_id = m.app_id
from migration_customer_apps m
where m.customer_id = c.id and m.app_rank = 1 and c.app_id is null;

insert into customers (
  id, workspace_id, app_id, app_user_id, apple_app_account_token,
  google_obfuscated_account_id, revenuecat_app_user_id, first_seen_at, last_seen_at
)
select
  'cus_migr_' || substr(md5(c.id || ':' || m.app_id), 1, 24),
  c.workspace_id,
  m.app_id,
  c.app_user_id,
  c.apple_app_account_token,
  c.google_obfuscated_account_id,
  c.revenuecat_app_user_id,
  c.first_seen_at,
  c.last_seen_at
from migration_customer_apps m
join customers c on c.id = m.customer_id
where m.app_rank > 1
on conflict (id) do nothing;

update transactions t
set customer_id = 'cus_migr_' || substr(md5(t.customer_id || ':' || t.app_id), 1, 24)
from migration_customer_apps m
where m.customer_id = t.customer_id and m.app_id = t.app_id and m.app_rank > 1;

update subscriptions s
set customer_id = 'cus_migr_' || substr(md5(s.customer_id || ':' || s.app_id), 1, 24)
from migration_customer_apps m
where m.customer_id = s.customer_id and m.app_id = s.app_id and m.app_rank > 1;

update customers c
set app_id = 'app_legacy_' || substr(md5(c.workspace_id), 1, 20)
where app_id is null;

create unique index if not exists customers_app_identity_idx
on customers (app_id, customer_identity_key);

alter table data_sources alter column app_id set not null;
alter table source_apps alter column app_id set not null;
alter table logical_products alter column app_id set not null;
alter table source_products alter column app_id set not null;
alter table product_mappings alter column app_id set not null;
alter table raw_events alter column app_id set not null;
alter table normalized_events alter column app_id set not null;
alter table customers alter column app_id set not null;
alter table transactions alter column app_id set not null;
alter table subscriptions alter column app_id set not null;
alter table daily_metrics alter column app_id set not null;
alter table sync_runs alter column app_id set not null;

create index if not exists app_memberships_user_idx on app_memberships (user_id, app_id);
create index if not exists data_sources_app_idx on data_sources (app_id, created_at desc);
create index if not exists raw_events_app_occurred_idx on raw_events (app_id, occurred_at desc);
create index if not exists normalized_events_app_occurred_idx on normalized_events (app_id, occurred_at desc);
create index if not exists transactions_app_time_idx on transactions (app_id, purchase_time desc);
create index if not exists subscriptions_app_updated_idx on subscriptions (app_id, updated_at desc);
create index if not exists sync_runs_app_started_idx on sync_runs (app_id, started_at desc);
create index if not exists jobs_app_created_idx on jobs (app_id, created_at desc);

alter table transactions
  drop constraint if exists transactions_workspace_id_source_type_transaction_key_key;
create unique index if not exists transactions_app_source_transaction_idx
on transactions (app_id, source_type, transaction_key);

alter table subscriptions
  drop constraint if exists subscriptions_workspace_id_subscription_key_key;
create unique index if not exists subscriptions_app_key_idx
on subscriptions (app_id, subscription_key);

create or replace view metric_events as
select ranked.*
from (
  select ne.*,
         ds.source_type,
         row_number() over (
           partition by ne.app_id,
                        ne.environment,
                        ne.event_type,
                        coalesce(ne.transaction_key, ne.raw_event_id)
           order by case ds.source_type
                      when 'app_store' then 0
                      when 'google_play' then 0
                      when 'stripe' then 0
                      when 'paddle' then 0
                      when 'revenuecat' then 1
                      when 'custom_api' then 2
                      else 3
                    end,
                    ne.confidence desc,
                    ne.created_at asc
         ) as metric_rank
  from normalized_events ne
  join data_sources ds on ds.id = ne.data_source_id and ds.app_id = ne.app_id
) ranked
where ranked.metric_rank = 1;

create or replace view effective_app_permissions as
select a.id as app_id, a.owner_user_id as user_id, permission
from apps a
cross join unnest(array[
  'app.read', 'ledger.read', 'events.sensitive.read', 'export.run',
  'app.write', 'catalog.write', 'source.write', 'source.credentials.write',
  'jobs.run', 'members.manage'
]) permission
where a.deleted_at is null
union
select a.id, wu.user_id, permission
from apps a
join workspace_users wu on wu.workspace_id = a.workspace_id
cross join unnest(array[
  'app.read', 'ledger.read', 'events.sensitive.read', 'export.run',
  'app.write', 'catalog.write', 'source.write', 'source.credentials.write',
  'jobs.run', 'members.manage'
]) permission
where a.deleted_at is null and wu.status = 'active' and wu.role in ('owner', 'admin')
union
select am.app_id, am.user_id, arp.permission
from app_memberships am
join app_role_permissions arp on arp.role_id = am.role_id
join apps a on a.id = am.app_id and a.deleted_at is null;

create or replace function has_app_permission(
  requested_user_id text,
  requested_app_id text,
  requested_permission text
) returns boolean
language sql
stable
security invoker
set search_path = public
as $$
  select exists (
    select 1
    from effective_app_permissions eap
    where eap.user_id = requested_user_id
      and eap.app_id = requested_app_id
      and eap.permission = requested_permission
  )
$$;
