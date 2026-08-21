import { RevternApi } from '@revtern/api-client';
import type { MeResponse } from '@revtern/types';
import * as SecureStore from 'expo-secure-store';
import { createContext, type ReactNode, useContext, useEffect, useMemo, useState } from 'react';

const SERVER_KEY = 'revtern.server-url';
const TOKEN_KEY = 'revtern.access-token';

type AuthContextValue = {
  api: RevternApi | null;
  connected: boolean;
  profile: MeResponse | null;
  ready: boolean;
  serverUrl: string | null;
  connect: (input: { serverUrl: string; email: string; password: string }) => Promise<void>;
  disconnect: () => Promise<void>;
};

const AuthContext = createContext<AuthContextValue | null>(null);

export function AuthProvider({ children }: { children: ReactNode }) {
  const [serverUrl, setServerUrl] = useState<string | null>(null);
  const [token, setToken] = useState<string | null>(null);
  const [profile, setProfile] = useState<MeResponse | null>(null);
  const [ready, setReady] = useState(false);

  const api = useMemo(
    () =>
      serverUrl
        ? new RevternApi({ baseUrl: serverUrl, accessToken: () => token })
        : null,
    [serverUrl, token],
  );

  useEffect(() => {
    let active = true;
    void (async () => {
      const [storedServer, storedToken] = await Promise.all([
        SecureStore.getItemAsync(SERVER_KEY),
        SecureStore.getItemAsync(TOKEN_KEY),
      ]);
      if (!active) return;
      if (!storedServer) {
        setReady(true);
        return;
      }
      const restoredApi = new RevternApi({
        baseUrl: storedServer,
        accessToken: () => storedToken,
      });
      try {
        const me = await restoredApi.me();
        if (!active) return;
        setServerUrl(storedServer);
        setToken(storedToken);
        setProfile(me);
      } catch {
        await Promise.all([
          SecureStore.deleteItemAsync(SERVER_KEY),
          SecureStore.deleteItemAsync(TOKEN_KEY),
        ]);
      } finally {
        if (active) setReady(true);
      }
    })();
    return () => {
      active = false;
    };
  }, []);

  async function connect(input: { serverUrl: string; email: string; password: string }) {
    const normalizedUrl = normalizeServerUrl(input.serverUrl);
    const publicApi = new RevternApi(normalizedUrl);
    const setup = await publicApi.setupStatus();
    if (setup.needs_setup) {
      throw new Error('Finish first-run setup in the Revtern web dashboard, then return here.');
    }

    let nextToken: string | null = null;
    if (setup.auth_mode === 'single_user') {
      if (!input.email.trim() || !input.password) {
        throw new Error('Email and password are required for this server.');
      }
      const session = await publicApi.mobileLogin({
        email: input.email.trim(),
        password: input.password,
      });
      nextToken = session.access_token;
    } else if (setup.auth_mode === 'reverse_proxy') {
      throw new Error('Reverse-proxy servers need mobile pairing, which is not enabled in this build.');
    }

    const nextApi = new RevternApi({
      baseUrl: normalizedUrl,
      accessToken: () => nextToken,
    });
    const me = await nextApi.me();
    await SecureStore.setItemAsync(SERVER_KEY, normalizedUrl);
    if (nextToken) await SecureStore.setItemAsync(TOKEN_KEY, nextToken);
    else await SecureStore.deleteItemAsync(TOKEN_KEY);
    setServerUrl(normalizedUrl);
    setToken(nextToken);
    setProfile(me);
  }

  async function disconnect() {
    try {
      if (api && token) await api.mobileLogout();
    } catch {
      // Local sign-out must still work when the server is offline.
    }
    await Promise.all([
      SecureStore.deleteItemAsync(SERVER_KEY),
      SecureStore.deleteItemAsync(TOKEN_KEY),
    ]);
    setProfile(null);
    setToken(null);
    setServerUrl(null);
  }

  const value: AuthContextValue = {
    api,
    connected: Boolean(serverUrl && profile),
    profile,
    ready,
    serverUrl,
    connect,
    disconnect,
  };
  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

export function useAuth() {
  const value = useContext(AuthContext);
  if (!value) throw new Error('useAuth must be used inside AuthProvider');
  return value;
}

export function useApi() {
  const { api } = useAuth();
  if (!api) throw new Error('Revtern API is not connected');
  return api;
}

function normalizeServerUrl(value: string) {
  const trimmed = value.trim().replace(/\/+$/, '');
  if (!trimmed) throw new Error('Enter the address of your Revtern server.');
  const withProtocol = /^https?:\/\//i.test(trimmed) ? trimmed : `https://${trimmed}`;
  const parsed = new URL(withProtocol);
  if (!['http:', 'https:'].includes(parsed.protocol)) {
    throw new Error('The server address must use HTTP or HTTPS.');
  }
  return parsed.toString().replace(/\/$/, '');
}
