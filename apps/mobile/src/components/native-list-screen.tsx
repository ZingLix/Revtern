import { Host } from '@expo/ui';
import type { ReactNode } from 'react';
import { StyleSheet, View } from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';

import { useColorScheme } from '@/hooks/use-color-scheme';
import { useTheme } from '@/hooks/use-theme';
import { ThemedText } from './themed-text';

export function NativeListScreen({
  title,
  subtitle,
  children,
}: {
  title: string;
  subtitle?: string;
  children: ReactNode;
}) {
  const theme = useTheme();
  const scheme = useColorScheme();
  return (
    <SafeAreaView edges={['top']} style={[styles.safeArea, { backgroundColor: theme.background }]}>
      <View style={[styles.header, { borderBottomColor: theme.separator }]}>
        <ThemedText type="title">{title}</ThemedText>
        {subtitle ? <ThemedText themeColor="textSecondary" type="small">{subtitle}</ThemedText> : null}
      </View>
      <Host
        colorScheme={scheme === 'unspecified' ? 'light' : scheme}
        seedColor={theme.accent}
        style={styles.host}
        useViewportSizeMeasurement>
        {children}
      </Host>
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  safeArea: { flex: 1 },
  header: {
    minHeight: 66,
    paddingHorizontal: 16,
    paddingVertical: 10,
    borderBottomWidth: StyleSheet.hairlineWidth,
    justifyContent: 'center',
    gap: 1,
  },
  host: { flex: 1, width: '100%' },
});
