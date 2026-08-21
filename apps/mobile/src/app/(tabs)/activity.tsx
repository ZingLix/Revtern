import { List, ListItem, Text as NativeText } from '@expo/ui';
import { useQuery } from '@tanstack/react-query';
import { useRouter } from 'expo-router';

import { NativeListScreen } from '@/components/native-list-screen';
import { useTheme } from '@/hooks/use-theme';
import { formatDateTime, formatMoney, formatStatus, last30Days } from '@/lib/format';
import { useApi } from '@/providers/auth';

const period = last30Days();

export default function ActivityScreen() {
  const api = useApi();
  const router = useRouter();
  const theme = useTheme();
  const transactions = useQuery({
    queryKey: ['transactions', period],
    queryFn: () => api.transactions(period),
  });
  const items = transactions.data?.transactions ?? [];

  return (
    <NativeListScreen subtitle="Production and verified events" title="Activity">
      <List onRefresh={async () => void (await transactions.refetch())}>
        {transactions.error ? (
          <ListItem supportingText={transactions.error.message}>Couldn’t load transactions</ListItem>
        ) : null}
        {!transactions.isLoading && !transactions.error && !items.length ? (
          <ListItem supportingText="New purchases, renewals, and refunds will appear here.">
            No transaction activity
          </ListItem>
        ) : null}
        {items.map((transaction) => (
          <ListItem
            key={transaction.id}
            onPress={() => router.push({ pathname: '/transaction/[id]', params: { id: transaction.id } })}
            supportingText={`${formatStatus(transaction.status)} · ${formatDateTime(transaction.purchase_time)}`}
            trailing={
              <NativeText textStyle={{ color: theme.text, fontSize: 14, fontWeight: '600' }}>
                {formatMoney(transaction.amount_minor, transaction.currency)}
              </NativeText>
            }>
            {transaction.logical_product_name ?? transaction.source_product_name ?? 'Unmapped product'}
          </ListItem>
        ))}
      </List>
    </NativeListScreen>
  );
}
