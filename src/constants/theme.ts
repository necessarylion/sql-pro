/** What the user picks. `system` follows the setting of the OS. */
export type ThemeChoice = 'dark' | 'light' | 'pink' | 'system'

/** The look that is really shown. */
export type Theme = 'dark' | 'light' | 'pink'

export const THEME_STORAGE_KEY = 'sql-pro.theme'

export const DEFAULT_THEME: ThemeChoice = 'dark'

/** The order here is the order of the buttons in the settings. */
export const THEME_LABELS: Record<ThemeChoice, string> = {
  dark: 'Dark',
  light: 'Light',
  pink: 'Pink',
  system: 'System',
}

export const LIGHT_SCHEME_QUERY = '(prefers-color-scheme: light)'
