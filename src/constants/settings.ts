import type { ThemeChoice } from './theme'

/** Sent by the Settings window after each change. Every window gets it. */
export const SETTINGS_CHANGED_EVENT = 'settings-changed'

/** All settings at once. The event carries them, so the main window needs no read of localStorage. */
export interface SettingsSnapshot {
  theme: ThemeChoice
  uiFont: string
  monoFont: string
  size: number
}
