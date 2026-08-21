import { NativeTabs } from 'expo-router/unstable-native-tabs';

import { useTheme } from '@/hooks/use-theme';

export default function TabLayout() {
  const theme = useTheme();
  return (
    <NativeTabs
      backgroundColor={theme.background}
      backBehavior="history"
      disableTransparentOnScrollEdge
      iconColor={{ default: theme.textSecondary, selected: theme.accent }}
      indicatorColor={theme.accentSoft}
      labelStyle={{ default: { color: theme.textSecondary }, selected: { color: theme.text } }}
      labelVisibilityMode="labeled"
      minimizeBehavior="onScrollDown"
      tintColor={theme.accent}>
      <NativeTabs.Trigger name="home">
        <NativeTabs.Trigger.Icon md="home" sf={{ default: 'house', selected: 'house.fill' }} />
        <NativeTabs.Trigger.Label>Overview</NativeTabs.Trigger.Label>
      </NativeTabs.Trigger>
      <NativeTabs.Trigger name="revenue">
        <NativeTabs.Trigger.Icon md="monitoring" sf="chart.xyaxis.line" />
        <NativeTabs.Trigger.Label>Revenue</NativeTabs.Trigger.Label>
      </NativeTabs.Trigger>
      <NativeTabs.Trigger name="activity">
        <NativeTabs.Trigger.Icon md="receipt_long" sf="list.bullet.rectangle" />
        <NativeTabs.Trigger.Label>Activity</NativeTabs.Trigger.Label>
      </NativeTabs.Trigger>
      <NativeTabs.Trigger name="subscriptions">
        <NativeTabs.Trigger.Icon md="subscriptions" sf="rectangle.stack" />
        <NativeTabs.Trigger.Label>Subs</NativeTabs.Trigger.Label>
      </NativeTabs.Trigger>
      <NativeTabs.Trigger name="settings">
        <NativeTabs.Trigger.Icon md="settings" sf="gearshape" />
        <NativeTabs.Trigger.Label>More</NativeTabs.Trigger.Label>
      </NativeTabs.Trigger>
    </NativeTabs>
  );
}
