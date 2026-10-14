export type ShortcutAction = 'save' | 'refresh'

/** The key that goes with Cmd (macOS) or Ctrl (Windows, Linux) for each action. */
export const SHORTCUT_KEYS: Record<string, ShortcutAction> = {
  s: 'save',
  r: 'refresh',
}

/** `navigator.platform` of an Apple device matches this. */
export const MAC_PLATFORM_PATTERN = /mac|iphone|ipad/i
