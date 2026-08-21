import { Redirect } from 'expo-router';
import { ActivityIndicator, StyleSheet, View } from 'react-native';

import { useTheme } from '@/hooks/use-theme';
import { useAuth } from '@/providers/auth';

export default function Index() {
  const { connected, ready } = useAuth();
  const theme = useTheme();
  if (!ready) {
    return (
      <View style={[styles.loading, { backgroundColor: theme.background }]}>
        <ActivityIndicator color={theme.accent} />
      </View>
    );
  }
  return <Redirect href={connected ? '/(tabs)/home' : '/connect'} />;
}

const styles = StyleSheet.create({
  loading: { flex: 1, alignItems: 'center', justifyContent: 'center' },
});
