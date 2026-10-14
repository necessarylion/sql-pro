export type ShortcutAction = 'save' | 'refresh' | 'export'

/** The key that goes with Cmd (macOS) or Ctrl (Windows, Linux) for each action. */
export const SHORTCUT_KEYS: Record<string, ShortcutAction> = {
  s: 'save',
  r: 'refresh',
}

/** The key that goes with Shift and Cmd (macOS) or Shift and Ctrl (Windows, Linux). */
export const SHIFT_SHORTCUT_KEYS: Record<string, ShortcutAction> = {
  e: 'export',
}

/** `navigator.platform` of an Apple device matches this. */
export const MAC_PLATFORM_PATTERN = /mac|iphone|ipad/i
