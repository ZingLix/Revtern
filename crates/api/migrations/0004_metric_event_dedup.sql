create or replace view metric_events as
select ranked.*
from (
  select ne.*,
         ds.source_type,
         row_number() over (
           partition by ne.workspace_id,
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
  join data_sources ds on ds.id = ne.data_source_id
) ranked
where ranked.metric_rank = 1;
