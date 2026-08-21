import { ActivityIndicator, StyleSheet, View } from 'react-native';

import { Spacing } from '@/constants/theme';
import { useTheme } from '@/hooks/use-theme';
import { ThemedText } from './themed-text';

export function LoadingState({ label = 'Loading' }: { label?: string }) {
  const theme = useTheme();
  return (
    <View style={styles.state}>
      <ActivityIndicator color={theme.accent} />
      <ThemedText themeColor="textSecondary" type="small">{label}</ThemedText>
    </View>
  );
}

export function EmptyState({ title, detail }: { title: string; detail: string }) {
  return (
    <View style={styles.state}>
      <ThemedText type="smallBold">{title}</ThemedText>
      <ThemedText style={styles.centered} themeColor="textSecondary" type="small">{detail}</ThemedText>
    </View>
  );
}

export function ErrorState({ error }: { error: unknown }) {
  const message = error instanceof Error ? error.message : 'Something went wrong.';
  return (
    <View style={styles.state}>
      <ThemedText themeColor="negative" type="smallBold">Couldn’t load this data</ThemedText>
      <ThemedText style={styles.centered} themeColor="textSecondary" type="small">{message}</ThemedText>
    </View>
  );
}

const styles = StyleSheet.create({
  state: {
    minHeight: 132,
    alignItems: 'center',
    justifyContent: 'center',
    gap: Spacing.two,
    paddingHorizontal: Spacing.four,
  },
  centered: { textAlign: 'center' },
});
