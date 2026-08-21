import { List, ListItem, Text as NativeText } from '@expo/ui';
import { useQuery } from '@tanstack/react-query';

import { NativeListScreen } from '@/components/native-list-screen';
import { useTheme } from '@/hooks/use-theme';
import { formatDate, formatStatus } from '@/lib/format';
import { useApi } from '@/providers/auth';

export default function SubscriptionsScreen() {
  const api = useApi();
  const theme = useTheme();
  const subscriptions = useQuery({
    queryKey: ['subscriptions'],
    queryFn: () => api.subscriptions({ environment: 'production' }),
  });
  const items = subscriptions.data?.subscriptions ?? [];

  return (
    <NativeListScreen subtitle={`${items.length} production subscriptions`} title="Subscriptions">
      <List onRefresh={async () => void (await subscriptions.refetch())}>
        {subscriptions.error ? (
          <ListItem supportingText={subscriptions.error.message}>Couldn’t load subscriptions</ListItem>
        ) : null}
        {!subscriptions.isLoading && !subscriptions.error && !items.length ? (
          <ListItem supportingText="Active and historical subscriptions will appear after lifecycle events arrive.">
            No subscriptions yet
          </ListItem>
        ) : null}
        {items.map((subscription) => {
          const periodEnd = subscription.current_period_end
            ? ` · ${subscription.will_renew ? 'Renews' : 'Ends'} ${formatDate(subscription.current_period_end)}`
            : '';
          return (
            <ListItem
              key={subscription.id}
              supportingText={`${formatStatus(subscription.status)}${periodEnd}`}
              trailing={
                <NativeText
                  textStyle={{
                    color: subscription.in_billing_retry ? theme.warning : theme.textSecondary,
                    fontSize: 13,
                    fontWeight: '500',
                  }}>
                  {subscription.in_billing_retry
                    ? 'Billing retry'
                    : subscription.in_grace_period
                      ? 'Grace period'
                      : subscription.will_renew
                        ? 'Auto-renew'
                        : 'No renewal'}
                </NativeText>
              }>
              {subscription.logical_product_name ?? subscription.source_product_name ?? 'Unmapped product'}
            </ListItem>
          );
        })}
      </List>
    </NativeListScreen>
  );
}
