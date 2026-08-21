import { useQuery } from '@tanstack/react-query';
import { useRouter } from 'expo-router';
import { Pressable, StyleSheet, View } from 'react-native';

import { RevenueChart } from '@/components/revenue-chart';
import { Divider, Screen, Section } from '@/components/screen';
import { EmptyState, ErrorState, LoadingState } from '@/components/states';
import { ThemedText } from '@/components/themed-text';
import { Radii, Spacing } from '@/constants/theme';
import { useTheme } from '@/hooks/use-theme';
import {
  formatDateTime,
  formatMoney,
  formatNumber,
  formatPercent,
  formatStatus,
  last30Days,
  trustLabel,
} from '@/lib/format';
import { useApi } from '@/providers/auth';

const period = last30Days();

export default function HomeScreen() {
  const api = useApi();
  const router = useRouter();
  const theme = useTheme();
  const filters = { ...period };
  const overview = useQuery({ queryKey: ['overview', filters], queryFn: () => api.overview(filters) });
  const series = useQuery({
    queryKey: ['revenue-series', filters],
    queryFn: () => api.revenueTimeseries(filters),
  });
  const transactions = useQuery({
    queryKey: ['transactions', 'recent', filters],
    queryFn: () => api.transactions(filters),
  });

  const refreshing = overview.isRefetching || series.isRefetching || transactions.isRefetching;
  function refresh() {
    void Promise.all([overview.refetch(), series.refetch(), transactions.refetch()]);
  }

  return (
    <Screen
      onRefresh={refresh}
      refreshing={refreshing}
      subtitle="Last 30 days"
      title="Overview">
      {overview.isLoading ? <LoadingState label="Loading overview" /> : null}
      {overview.error ? <ErrorState error={overview.error} /> : null}
      {overview.data ? (
        <>
          <View style={styles.primaryMetric}>
            <View style={styles.metricHeading}>
              <ThemedText themeColor="textSecondary" type="small">Gross revenue</ThemedText>
              <View style={[styles.trust, { backgroundColor: theme.accentSoft }]}>
                <ThemedText style={{ color: theme.accent }} type="caption">
                  {trustLabel(overview.data.metrics.gross_revenue_minor.trust_state)}
                </ThemedText>
              </View>
            </View>
            <ThemedText type="metric">
              {formatMoney(
                overview.data.metrics.gross_revenue_minor.value,
                overview.data.currency,
              )}
            </ThemedText>
            {series.data?.series.length ? <RevenueChart series={series.data.series} /> : null}
          </View>

          {overview.data.warnings.length ? (
            <View style={[styles.warning, { borderColor: theme.separator }]}>
              <ThemedText themeColor="warning" type="smallBold">Data needs attention</ThemedText>
              <ThemedText themeColor="textSecondary" type="small">
                {overview.data.warnings[0]}
              </ThemedText>
            </View>
          ) : null}

          <Section title="Subscription movement">
            <View style={styles.rows}>
              <MetricRow label="Active" value={formatNumber(overview.data.metrics.active_subscriptions.value)} />
              <Divider />
              <MetricRow label="New" value={formatNumber(overview.data.metrics.new_subscriptions.value)} />
              <Divider />
              <MetricRow label="Renewals" value={formatNumber(overview.data.metrics.renewals.value)} />
              <Divider />
              <MetricRow label="Churned" value={formatNumber(overview.data.metrics.churned_subscriptions.value)} />
            </View>
          </Section>

          <Section title="Revenue quality">
            <View style={styles.rows}>
              <MetricRow
                label="Net revenue"
                value={formatMoney(overview.data.metrics.net_revenue_minor.value, overview.data.currency)}
              />
              <Divider />
              <MetricRow
                label="Refunds"
                value={formatMoney(overview.data.metrics.refund_amount_minor.value, overview.data.currency)}
              />
              <Divider />
              <MetricRow label="Refund rate" value={formatPercent(overview.data.metrics.refund_rate.value)} />
            </View>
          </Section>
        </>
      ) : null}

      <Section title="Recent transactions">
        {transactions.isLoading ? <LoadingState label="Loading transactions" /> : null}
        {transactions.error ? <ErrorState error={transactions.error} /> : null}
        {transactions.data && !transactions.data.transactions.length ? (
          <EmptyState
            detail="Transactions will appear after a connected source sends purchase events."
            title="No transactions yet"
          />
        ) : null}
        {transactions.data?.transactions.slice(0, 6).map((transaction, index) => (
          <View key={transaction.id}>
            {index ? <Divider /> : null}
            <Pressable
              accessibilityRole="button"
              onPress={() => router.push({ pathname: '/transaction/[id]', params: { id: transaction.id } })}
              style={({ pressed }) => [styles.transaction, pressed && { backgroundColor: theme.backgroundElement }]}>
              <View style={styles.transactionText}>
                <ThemedText numberOfLines={1} type="smallBold">
                  {transaction.logical_product_name ?? transaction.source_product_name ?? 'Unmapped product'}
                </ThemedText>
                <ThemedText numberOfLines={1} themeColor="textSecondary" type="caption">
                  {formatStatus(transaction.status)} · {formatDateTime(transaction.purchase_time)}
                </ThemedText>
              </View>
              <ThemedText type="smallBold">
                {formatMoney(transaction.amount_minor, transaction.currency)}
              </ThemedText>
            </Pressable>
          </View>
        ))}
      </Section>
    </Screen>
  );
}

function MetricRow({ label, value }: { label: string; value: string }) {
  return (
    <View style={styles.metricRow}>
      <ThemedText themeColor="textSecondary" type="small">{label}</ThemedText>
      <ThemedText type="smallBold">{value}</ThemedText>
    </View>
  );
}

const styles = StyleSheet.create({
  primaryMetric: { gap: 12 },
  metricHeading: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between' },
  trust: { paddingHorizontal: 8, paddingVertical: 4, borderRadius: Radii.compact },
  warning: { borderWidth: StyleSheet.hairlineWidth, borderRadius: Radii.panel, padding: 12, gap: 4 },
  rows: { gap: 0 },
  metricRow: { minHeight: 44, flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between' },
  transaction: {
    minHeight: 58,
    paddingVertical: 9,
    paddingHorizontal: 4,
    borderRadius: 8,
    flexDirection: 'row',
    alignItems: 'center',
    gap: Spacing.three,
  },
  transactionText: { flex: 1, gap: 2 },
});
