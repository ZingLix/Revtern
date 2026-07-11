alter table raw_events
  add column if not exists environment text not null default 'unknown';

alter table normalized_events
  add column if not exists environment text not null default 'unknown';

alter table transactions
  add column if not exists environment text not null default 'unknown';

alter table subscriptions
  add column if not exists environment text not null default 'unknown';

create index if not exists raw_events_environment_idx
on raw_events (workspace_id, environment, occurred_at desc);

create index if not exists normalized_events_environment_idx
on normalized_events (workspace_id, environment, occurred_at desc);

create index if not exists transactions_environment_idx
on transactions (workspace_id, environment, purchase_time desc);

create index if not exists subscriptions_environment_idx
on subscriptions (workspace_id, environment, updated_at desc);
