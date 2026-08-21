import { Button, Host, Picker, TextInput } from '@expo/ui';
import type { ReactNode } from 'react';
import { StyleSheet, View } from 'react-native';

import { Radii } from '@/constants/theme';
import { useColorScheme } from '@/hooks/use-color-scheme';
import { useTheme } from '@/hooks/use-theme';

function useNativeTheme() {
  const theme = useTheme();
  const scheme = useColorScheme();
  return { theme, scheme: scheme === 'unspecified' ? 'light' : scheme } as const;
}

export function NativeButton({
  label,
  onPress,
  disabled,
  variant = 'filled',
}: {
  label: string;
  onPress: () => void;
  disabled?: boolean;
  variant?: 'filled' | 'outlined' | 'text';
}) {
  const { theme, scheme } = useNativeTheme();
  return (
    <Host
      colorScheme={scheme}
      matchContents={{ vertical: true, horizontal: false }}
      seedColor={theme.accent}
      style={styles.host}>
      <Button
        disabled={disabled}
        label={label}
        onPress={onPress}
        style={styles.button}
        variant={variant}
      />
    </Host>
  );
}

export function NativeTextField({
  defaultValue,
  onChangeText,
  placeholder,
  secureTextEntry,
  keyboardType,
  autoComplete,
  onSubmitEditing,
}: {
  defaultValue?: string;
  onChangeText: (text: string) => void;
  placeholder: string;
  secureTextEntry?: boolean;
  keyboardType?: 'default' | 'email-address' | 'url';
  autoComplete?: 'email' | 'password' | 'url';
  onSubmitEditing?: (text: string) => void;
}) {
  const { theme, scheme } = useNativeTheme();
  return (
    <Host
      colorScheme={scheme}
      matchContents={{ vertical: true, horizontal: false }}
      seedColor={theme.accent}
      style={styles.host}>
      <TextInput
        autoCapitalize="none"
        autoComplete={autoComplete}
        autoCorrect={false}
        defaultValue={defaultValue}
        keyboardType={keyboardType}
        onChangeText={onChangeText}
        onSubmitEditing={onSubmitEditing}
        placeholder={placeholder}
        placeholderTextColor={theme.textSecondary}
        secureTextEntry={secureTextEntry}
        style={{
          width: '100%',
          height: 48,
          paddingHorizontal: 13,
          backgroundColor: theme.backgroundElement,
          borderColor: theme.separator,
          borderWidth: StyleSheet.hairlineWidth,
          borderRadius: Radii.control,
        }}
        textStyle={{ color: theme.text, fontSize: 16 }}
      />
    </Host>
  );
}

export function NativePicker<T extends string>({
  value,
  onChange,
  children,
}: {
  value: T;
  onChange: (value: T) => void;
  children: ReactNode;
}) {
  const { theme, scheme } = useNativeTheme();
  return (
    <View style={styles.pickerFrame}>
      <Host colorScheme={scheme} matchContents seedColor={theme.accent}>
        <Picker appearance="menu" onValueChange={(next) => onChange(next as T)} selectedValue={value}>
          {children}
        </Picker>
      </Host>
    </View>
  );
}

export const NativePickerItem = Picker.Item;

const styles = StyleSheet.create({
  host: { alignSelf: 'stretch', width: '100%' },
  button: { width: '100%', height: 46, borderRadius: Radii.control },
  pickerFrame: { minWidth: 132, minHeight: 40, justifyContent: 'center' },
});
