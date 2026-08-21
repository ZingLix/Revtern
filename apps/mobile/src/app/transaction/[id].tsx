import { useQuery } from '@tanstack/react-query';
import { useLocalSearchParams } from 'expo-router';
import { ScrollView, StyleSheet, View } from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';

import { Divider, Section } from '@/components/screen';
import { ErrorState, LoadingState } from '@/components/states';
import { ThemedText } from '@/components/themed-text';
import { Spacing } from '@/constants/theme';
import { useTheme } from '@/hooks/use-theme';
import { formatDateTime, formatMoney, formatStatus } from '@/lib/format';
import { useApi } from '@/providers/auth';

export default function TransactionDetailScreen() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const api = useApi();
  const theme = useTheme();
  const detail = useQuery({
    queryKey: ['transaction', id],
    queryFn: () => api.transaction(id),
    enabled: Boolean(id),
  });
  const transaction = detail.data?.transaction;

  return (
    <SafeAreaView edges={['bottom']} style={[styles.safeArea, { backgroundColor: theme.background }]}>
      <ScrollView contentContainerStyle={styles.content}>
        {detail.isLoading ? <LoadingState label="Loading transaction" /> : null}
        {detail.error ? <ErrorState error={detail.error} /> : null}
        {transaction ? (
          <>
            <View style={styles.amount}>
              <ThemedText themeColor="textSecondary" type="small">
                {transaction.logical_product_name ?? transaction.source_product_name ?? 'Unmapped product'}
              </ThemedText>
              <ThemedText type="metric">{formatMoney(transaction.amount_minor, transaction.currency)}</ThemedText>
              <ThemedText themeColor="textSecondary" type="small">
                {formatStatus(transaction.status)} · {formatDateTime(transaction.purchase_time)}
              </ThemedText>
            </View>

            <Section title="Details">
              <DetailRow label="App" value={transaction.app_name ?? 'Unassigned'} />
              <Divider />
              <DetailRow label="Platform" value={formatStatus(transaction.platform ?? 'unknown')} />
              <Divider />
              <DetailRow label="Source" value={formatStatus(transaction.source_type)} />
              <Divider />
              <DetailRow label="Environment" value={formatStatus(transaction.environment)} />
              <Divider />
              <DetailRow label="Country" value={transaction.country ?? 'Unknown'} />
              <Divider />
              <DetailRow label="Transaction ID" value={transaction.transaction_key} />
            </Section>

            <Section title="Evidence timeline">
              {detail.data?.events.map((event, index) => (
                <View key={event.id}>
                  {index ? <Divider /> : null}
                  <View style={styles.eventRow}>
                    <View style={styles.eventText}>
                      <ThemedText type="smallBold">{formatStatus(event.event_type)}</ThemedText>
                      <ThemedText themeColor="textSecondary" type="caption">
                        {formatDateTime(event.occurred_at)}
                      </ThemedText>
                    </View>
                    {event.amount_minor != null ? (
                      <ThemedText type="smallBold">{formatMoney(event.amount_minor, event.currency ?? transaction.currency)}</ThemedText>
                    ) : null}
                  </View>
                </View>
              ))}
            </Section>
          </>
        ) : null}
      </ScrollView>
    </SafeAreaView>
  );
}

function DetailRow({ label, value }: { label: string; value: string }) {
  return (
    <View style={styles.detailRow}>
      <ThemedText themeColor="textSecondary" type="small">{label}</ThemedText>
      <ThemedText numberOfLines={2} style={styles.detailValue} type="smallBold">{value}</ThemedText>
    </View>
  );
}

const styles = StyleSheet.create({
  safeArea: { flex: 1 },
  content: { padding: Spacing.three, paddingBottom: Spacing.six, gap: Spacing.four },
  amount: { gap: 4, paddingVertical: Spacing.two },
  detailRow: { minHeight: 46, flexDirection: 'row', alignItems: 'center', gap: 16 },
  detailValue: { flex: 1, textAlign: 'right' },
  eventRow: { minHeight: 56, flexDirection: 'row', alignItems: 'center', gap: 16 },
  eventText: { flex: 1, gap: 2 },
});
