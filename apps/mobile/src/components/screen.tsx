import type { ReactNode } from 'react';
import { RefreshControl, ScrollView, StyleSheet, View } from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';

import { Spacing } from '@/constants/theme';
import { useTheme } from '@/hooks/use-theme';
import { ThemedText } from './themed-text';

export function Screen({
  title,
  subtitle,
  children,
  refreshing = false,
  onRefresh,
  action,
}: {
  title: string;
  subtitle?: string;
  children: ReactNode;
  refreshing?: boolean;
  onRefresh?: () => void;
  action?: ReactNode;
}) {
  const theme = useTheme();
  return (
    <SafeAreaView edges={['top']} style={[styles.safeArea, { backgroundColor: theme.background }]}>
      <View style={[styles.header, { borderBottomColor: theme.separator }]}>
        <View style={styles.heading}>
          <ThemedText type="title">{title}</ThemedText>
          {subtitle ? (
            <ThemedText numberOfLines={1} themeColor="textSecondary" type="small">
              {subtitle}
            </ThemedText>
          ) : null}
        </View>
        {action}
      </View>
      <ScrollView
        contentContainerStyle={styles.content}
        refreshControl={
          onRefresh ? (
            <RefreshControl
              colors={[theme.accent]}
              onRefresh={onRefresh}
              refreshing={refreshing}
              tintColor={theme.accent}
            />
          ) : undefined
        }>
        {children}
      </ScrollView>
    </SafeAreaView>
  );
}

export function Section({ title, action, children }: { title: string; action?: ReactNode; children: ReactNode }) {
  return (
    <View style={styles.section}>
      <View style={styles.sectionHeader}>
        <ThemedText type="sectionTitle">{title}</ThemedText>
        {action}
      </View>
      {children}
    </View>
  );
}

export function Divider() {
  const theme = useTheme();
  return <View style={[styles.divider, { backgroundColor: theme.separator }]} />;
}

const styles = StyleSheet.create({
  safeArea: { flex: 1 },
  header: {
    minHeight: 66,
    paddingHorizontal: Spacing.three,
    paddingVertical: 10,
    borderBottomWidth: StyleSheet.hairlineWidth,
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    gap: 12,
  },
  heading: { flex: 1, gap: 1 },
  content: {
    paddingHorizontal: Spacing.three,
    paddingTop: Spacing.three,
    paddingBottom: 112,
    gap: Spacing.four,
  },
  section: { gap: 10 },
  sectionHeader: {
    minHeight: 28,
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    gap: 12,
  },
  divider: { height: StyleSheet.hairlineWidth, width: '100%' },
});
