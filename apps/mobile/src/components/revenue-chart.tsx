import type { DailyRevenuePoint } from '@revtern/types';
import { useMemo } from 'react';
import { StyleSheet, View } from 'react-native';
import Svg, { Line, Polyline } from 'react-native-svg';

import { useTheme } from '@/hooks/use-theme';
import { ThemedText } from './themed-text';

const WIDTH = 340;
const HEIGHT = 142;
const INSET = 8;

export function RevenueChart({ series }: { series: DailyRevenuePoint[] }) {
  const theme = useTheme();
  const geometry = useMemo(() => chartGeometry(series), [series]);
  if (!series.length) return null;
  return (
    <View accessibilityLabel="Gross revenue over time" style={styles.container}>
      <Svg height={HEIGHT} preserveAspectRatio="none" viewBox={`0 0 ${WIDTH} ${HEIGHT}`} width="100%">
        <Line
          stroke={theme.separator}
          strokeWidth={1}
          x1={INSET}
          x2={WIDTH - INSET}
          y1={HEIGHT - INSET}
          y2={HEIGHT - INSET}
        />
        <Polyline
          fill="none"
          points={geometry.points}
          stroke={theme.accent}
          strokeLinecap="round"
          strokeLinejoin="round"
          strokeWidth={2.5}
        />
      </Svg>
      <View style={styles.axisLabels}>
        <ThemedText themeColor="textSecondary" type="caption">{geometry.firstLabel}</ThemedText>
        <ThemedText themeColor="textSecondary" type="caption">{geometry.lastLabel}</ThemedText>
      </View>
    </View>
  );
}

function chartGeometry(series: DailyRevenuePoint[]) {
  const values = series.map((point) => point.gross_revenue_minor);
  const max = Math.max(...values, 1);
  const step = (WIDTH - INSET * 2) / Math.max(series.length - 1, 1);
  const points = values
    .map((value, index) => {
      const x = INSET + index * step;
      const y = HEIGHT - INSET - (value / max) * (HEIGHT - INSET * 2);
      return `${x},${y}`;
    })
    .join(' ');
  return {
    points,
    firstLabel: shortDate(series[0]?.date),
    lastLabel: shortDate(series.at(-1)?.date),
  };
}

function shortDate(value: string | undefined) {
  if (!value) return '';
  return new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric' }).format(
    new Date(`${value}T00:00:00`),
  );
}

const styles = StyleSheet.create({
  container: { width: '100%', gap: 4 },
  axisLabels: { flexDirection: 'row', justifyContent: 'space-between' },
});
