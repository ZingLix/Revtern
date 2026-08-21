import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useRouter } from 'expo-router';
import { StyleSheet, View } from 'react-native';

import { NativeButton, NativePicker, NativePickerItem } from '@/components/native-controls';
import { Divider, Screen, Section } from '@/components/screen';
import { ErrorState, LoadingState } from '@/components/states';
import { ThemedText } from '@/components/themed-text';
import { Radii } from '@/constants/theme';
import { useTheme } from '@/hooks/use-theme';
import { formatDateTime, formatStatus } from '@/lib/format';
import { useApi, useAuth } from '@/providers/auth';

export default function SettingsScreen() {
  const api = useApi();
  const { apps, profile, selectedAppId, selectApp, serverUrl, disconnect } = useAuth();
  const router = useRouter();
  const queryClient = useQueryClient();
  const theme = useTheme();
  const sources = useQuery({ queryKey: ['data-sources', selectedAppId], queryFn: () => api.dataSources({ app_id: selectedAppId ?? undefined }) });

  async function signOut() {
    await disconnect();
    queryClient.clear();
    router.replace('/connect');
  }

  return (
    <Screen
      onRefresh={() => void sources.refetch()}
      refreshing={sources.isRefetching}
      subtitle={profile?.workspace.name}
      title="More">
      <Section title="Account">
        <View style={styles.rows}>
          <InfoRow label="Owner" value={profile?.user.email ?? '—'} />
          <Divider />
          <InfoRow label="Role" value={formatStatus(profile?.user.role ?? 'owner')} />
          <Divider />
          <InfoRow label="Server" value={serverUrl ?? '—'} />
        </View>
      </Section>

      {selectedAppId ? (
        <Section title="Current app">
          <NativePicker value={selectedAppId} onChange={(appId) => void selectApp(appId)}>
            {apps.map((app) => <NativePickerItem key={app.id} label={`${app.name} · ${formatStatus(app.role)}`} value={app.id} />)}
          </NativePicker>
        </Section>
      ) : null}

      <Section title="Source health">
        {sources.isLoading ? <LoadingState label="Checking sources" /> : null}
        {sources.error ? <ErrorState error={sources.error} /> : null}
        {sources.data?.data_sources.map((source, index) => (
          <View key={source.id}>
            {index ? <Divider /> : null}
            <View style={styles.sourceRow}>
              <View style={styles.sourceText}>
                <ThemedText type="smallBold">{source.name}</ThemedText>
                <ThemedText numberOfLines={1} themeColor="textSecondary" type="caption">
                  {source.last_event_at ? `Last event ${formatDateTime(source.last_event_at)}` : 'Waiting for events'}
                </ThemedText>
              </View>
              <View style={[styles.status, { backgroundColor: theme.backgroundElement }]}>
                <ThemedText
                  themeColor={source.status === 'active' ? 'positive' : source.status === 'error' ? 'negative' : 'textSecondary'}
                  type="caption">
                  {formatStatus(source.status)}
                </ThemedText>
              </View>
            </View>
          </View>
        ))}
        {sources.data && !sources.data.data_sources.length ? (
          <ThemedText themeColor="textSecondary" type="small">
            Connect a source from the web dashboard to start receiving purchase data.
          </ThemedText>
        ) : null}
      </Section>

      <Section title="Session">
        <NativeButton label="Disconnect this device" onPress={() => void signOut()} variant="outlined" />
        <ThemedText themeColor="textSecondary" type="caption">
          This removes the server address and device token from this phone. Your Revtern data stays on the server.
        </ThemedText>
      </Section>
    </Screen>
  );
}

function InfoRow({ label, value }: { label: string; value: string }) {
  return (
    <View style={styles.infoRow}>
      <ThemedText themeColor="textSecondary" type="small">{label}</ThemedText>
      <ThemedText numberOfLines={1} style={styles.infoValue} type="smallBold">{value}</ThemedText>
    </View>
  );
}

const styles = StyleSheet.create({
  rows: { gap: 0 },
  infoRow: { minHeight: 46, flexDirection: 'row', alignItems: 'center', gap: 16 },
  infoValue: { flex: 1, textAlign: 'right' },
  sourceRow: { minHeight: 62, flexDirection: 'row', alignItems: 'center', gap: 12 },
  sourceText: { flex: 1, gap: 2 },
  status: { paddingHorizontal: 8, paddingVertical: 5, borderRadius: Radii.compact },
});
