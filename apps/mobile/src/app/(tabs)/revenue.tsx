import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { StyleSheet, View } from 'react-native';

import { NativePicker, NativePickerItem } from '@/components/native-controls';
import { RevenueChart } from '@/components/revenue-chart';
import { Divider, Screen, Section } from '@/components/screen';
import { EmptyState, ErrorState, LoadingState } from '@/components/states';
import { ThemedText } from '@/components/themed-text';
import { formatMoney, formatNumber, last30Days } from '@/lib/format';
import { useApi } from '@/providers/auth';

type Breakdown = 'product' | 'app' | 'platform' | 'country' | 'source';
const period = last30Days();

export default function RevenueScreen() {
  const api = useApi();
  const [by, setBy] = useState<Breakdown>('product');
  const series = useQuery({
    queryKey: ['revenue-series', period],
    queryFn: () => api.revenueTimeseries(period),
  });
  const breakdown = useQuery({
    queryKey: ['breakdown', period, by],
    queryFn: () => api.breakdown({ ...period, by }),
  });
  const total = series.data?.series.reduce((sum, point) => sum + point.gross_revenue_minor, 0) ?? 0;
  const refunds = series.data?.series.reduce((sum, point) => sum + point.refund_amount_minor, 0) ?? 0;
  const currency = 'USD';
  const refreshing = series.isRefetching || breakdown.isRefetching;

  return (
    <Screen
      onRefresh={() => void Promise.all([series.refetch(), breakdown.refetch()])}
      refreshing={refreshing}
      subtitle="Last 30 days · USD"
      title="Revenue">
      {series.isLoading ? <LoadingState label="Loading revenue" /> : null}
      {series.error ? <ErrorState error={series.error} /> : null}
      {series.data ? (
        <View style={styles.summary}>
          <ThemedText themeColor="textSecondary" type="small">Gross revenue</ThemedText>
          <ThemedText type="metric">{formatMoney(total, currency)}</ThemedText>
          {series.data.series.length ? (
            <RevenueChart series={series.data.series} />
          ) : (
            <EmptyState detail="Revenue will appear after production purchase events arrive." title="No revenue yet" />
          )}
          <View style={styles.inlineMetrics}>
            <View style={styles.inlineMetric}>
              <ThemedText themeColor="textSecondary" type="caption">Net</ThemedText>
              <ThemedText type="smallBold">{formatMoney(total - refunds, currency)}</ThemedText>
            </View>
            <View style={styles.inlineMetric}>
              <ThemedText themeColor="textSecondary" type="caption">Refunds</ThemedText>
              <ThemedText type="smallBold">{formatMoney(refunds, currency)}</ThemedText>
            </View>
            <View style={styles.inlineMetric}>
              <ThemedText themeColor="textSecondary" type="caption">Purchases</ThemedText>
              <ThemedText type="smallBold">
                {formatNumber(series.data.series.reduce((sum, point) => sum + point.purchase_count, 0))}
              </ThemedText>
            </View>
          </View>
        </View>
      ) : null}

      <Section
        action={
          <NativePicker onChange={setBy} value={by}>
            <NativePickerItem label="Product" value="product" />
            <NativePickerItem label="App" value="app" />
            <NativePickerItem label="Platform" value="platform" />
            <NativePickerItem label="Country" value="country" />
            <NativePickerItem label="Source" value="source" />
          </NativePicker>
        }
        title="Breakdown">
        {breakdown.isLoading ? <LoadingState /> : null}
        {breakdown.error ? <ErrorState error={breakdown.error} /> : null}
        {breakdown.data?.items.map((item, index) => (
          <View key={item.label}>
            {index ? <Divider /> : null}
            <View style={styles.breakdownRow}>
              <View style={styles.breakdownLabel}>
                <ThemedText numberOfLines={1} type="smallBold">{item.label}</ThemedText>
                <ThemedText themeColor="textSecondary" type="caption">
                  {formatNumber(item.transaction_count)} transactions
                </ThemedText>
              </View>
              <ThemedText type="smallBold">{formatMoney(item.gross_revenue_minor, currency)}</ThemedText>
            </View>
          </View>
        ))}
      </Section>
    </Screen>
  );
}

const styles = StyleSheet.create({
  summary: { gap: 10 },
  inlineMetrics: { flexDirection: 'row', gap: 20, paddingTop: 4 },
  inlineMetric: { flex: 1, gap: 2 },
  breakdownRow: { minHeight: 58, flexDirection: 'row', alignItems: 'center', gap: 16 },
  breakdownLabel: { flex: 1, gap: 2 },
});
