# Revtern Mobile

Native iOS and Android companion app for Revtern, built with Expo SDK 57,
React Native 0.86, Expo Router, and Expo UI.

## Scope

- Connect to a self-hosted Revtern server.
- Store the mobile bearer session in SecureStore.
- Overview and revenue trend.
- Revenue breakdowns.
- Transaction activity and evidence details.
- Subscription status.
- Data-source health and device disconnect.

Source configuration, product mapping, raw payload inspection, and job retries
remain in the web dashboard.

## Run

Start the Revtern API on port `3000`, then from the repository root:

```bash
npm run dev:mobile
```

Press `i` for the iOS Simulator or `a` for Android. Native development builds
can also be created with:

```bash
npm run ios -w @revtern/mobile
npm run android -w @revtern/mobile
```

The development connection screen uses `http://127.0.0.1:3000` on iOS Simulator
and `http://10.0.2.2:3000` on Android Emulator. Use an HTTPS URL with a valid
certificate for production builds and physical devices.

## Checks

```bash
npm run typecheck -w @revtern/mobile
npx expo-doctor
npx expo export --platform ios
npx expo export --platform android
```

## License

Revtern Mobile is licensed under the repository's
[Apache License 2.0](../../LICENSE). See
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for template attribution.
