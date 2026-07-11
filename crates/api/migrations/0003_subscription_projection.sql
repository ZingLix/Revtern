alter table subscriptions
  add column if not exists status_updated_at timestamptz not null default now();

create index if not exists subscriptions_workspace_updated_idx
on subscriptions (workspace_id, status_updated_at desc);
