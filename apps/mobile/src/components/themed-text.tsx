import { Platform, StyleSheet, Text, type TextProps } from 'react-native';

import { Fonts, type ThemeColor } from '@/constants/theme';
import { useTheme } from '@/hooks/use-theme';

export type ThemedTextProps = TextProps & {
  type?:
    | 'default'
    | 'title'
    | 'sectionTitle'
    | 'metric'
    | 'small'
    | 'smallBold'
    | 'caption'
    | 'subtitle'
    | 'link'
    | 'code';
  themeColor?: ThemeColor;
};

export function ThemedText({ style, type = 'default', themeColor, ...rest }: ThemedTextProps) {
  const theme = useTheme();
  return (
    <Text
      style={[
        { color: theme[themeColor ?? 'text'] },
        styles[type],
        style,
      ]}
      {...rest}
    />
  );
}

const styles = StyleSheet.create({
  default: { fontSize: 16, lineHeight: 23, fontWeight: 400 },
  title: { fontSize: 23, lineHeight: 29, fontWeight: 600 },
  sectionTitle: { fontSize: 16, lineHeight: 22, fontWeight: 600 },
  metric: {
    fontSize: 34,
    lineHeight: 40,
    fontWeight: 600,
    fontVariant: ['tabular-nums'],
  },
  subtitle: { fontSize: 20, lineHeight: 26, fontWeight: 600 },
  small: { fontSize: 14, lineHeight: 20, fontWeight: 400 },
  smallBold: { fontSize: 14, lineHeight: 20, fontWeight: 600 },
  caption: { fontSize: 12, lineHeight: 16, fontWeight: 500 },
  link: { fontSize: 14, lineHeight: 20, fontWeight: 500 },
  code: {
    fontFamily: Fonts.mono,
    fontWeight: Platform.select({ android: 700 }) ?? 500,
    fontSize: 12,
  },
});
