import { useRouter } from 'expo-router';
import { useState } from 'react';
import { KeyboardAvoidingView, Platform, ScrollView, StyleSheet, View } from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';

import { NativeButton, NativeTextField } from '@/components/native-controls';
import { ThemedText } from '@/components/themed-text';
import { Spacing } from '@/constants/theme';
import { useTheme } from '@/hooks/use-theme';
import { useAuth } from '@/providers/auth';

export default function ConnectScreen() {
  const { connect } = useAuth();
  const router = useRouter();
  const theme = useTheme();
  const [serverUrl, setServerUrl] = useState(
    __DEV__ ? (Platform.OS === 'android' ? 'http://10.0.2.2:3000' : 'http://127.0.0.1:3000') : '',
  );
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  async function submit() {
    setSubmitting(true);
    setError(null);
    try {
      await connect({ serverUrl, email, password });
      router.replace('/(tabs)/home');
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : 'Could not connect to this server.');
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <SafeAreaView style={[styles.safeArea, { backgroundColor: theme.background }]}>
      <KeyboardAvoidingView
        behavior={Platform.OS === 'ios' ? 'padding' : undefined}
        style={styles.flex}>
        <ScrollView
          contentContainerStyle={styles.scrollContent}
          keyboardShouldPersistTaps="handled">
          <View style={styles.form}>
            <View style={styles.brand}>
              <View style={[styles.mark, { borderColor: theme.separator }]}>
                <ThemedText style={{ color: theme.accent }} type="smallBold">R</ThemedText>
              </View>
              <ThemedText type="title">Revtern</ThemedText>
            </View>

            <View style={styles.intro}>
              <ThemedText type="subtitle">Connect your server</ThemedText>
              <ThemedText themeColor="textSecondary">
                Use the same server address and local account as your self-hosted dashboard.
              </ThemedText>
            </View>

            <View style={styles.fields}>
              <FieldLabel>Server address</FieldLabel>
              <NativeTextField
                autoComplete="url"
                defaultValue={serverUrl}
                keyboardType="url"
                onChangeText={setServerUrl}
                placeholder="https://revtern.example.com"
              />
              <FieldLabel>Email</FieldLabel>
              <NativeTextField
                autoComplete="email"
                keyboardType="email-address"
                onChangeText={setEmail}
                placeholder="owner@example.com"
              />
              <FieldLabel>Password</FieldLabel>
              <NativeTextField
                autoComplete="password"
                onChangeText={setPassword}
                onSubmitEditing={() => void submit()}
                placeholder="Password"
                secureTextEntry
              />
            </View>

            {error ? (
              <View style={[styles.error, { backgroundColor: theme.accentSoft }]}>
                <ThemedText themeColor="negative" type="small">{error}</ThemedText>
              </View>
            ) : null}

            <NativeButton
              disabled={submitting}
              label={submitting ? 'Connecting…' : 'Connect'}
              onPress={() => void submit()}
            />
            <ThemedText style={styles.note} themeColor="textSecondary" type="caption">
              Credentials are sent only to your server. The device session is stored in the system keychain.
            </ThemedText>
          </View>
        </ScrollView>
      </KeyboardAvoidingView>
    </SafeAreaView>
  );
}

function FieldLabel({ children }: { children: string }) {
  return <ThemedText type="smallBold">{children}</ThemedText>;
}

const styles = StyleSheet.create({
  flex: { flex: 1 },
  safeArea: { flex: 1 },
  scrollContent: {
    flexGrow: 1,
    justifyContent: 'center',
    paddingHorizontal: Spacing.four,
    paddingVertical: Spacing.five,
  },
  form: { width: '100%', maxWidth: 440, alignSelf: 'center', gap: Spacing.four },
  brand: { flexDirection: 'row', alignItems: 'center', gap: 10 },
  mark: {
    width: 32,
    height: 32,
    borderRadius: 8,
    borderWidth: StyleSheet.hairlineWidth,
    alignItems: 'center',
    justifyContent: 'center',
  },
  intro: { gap: Spacing.two },
  fields: { gap: 8 },
  error: { padding: 12, borderRadius: 8 },
  note: { textAlign: 'center', paddingHorizontal: Spacing.three },
});
