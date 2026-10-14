export type ShortcutAction = 'save' | 'refresh' | 'export' | 'detail'

/** The key that goes with Cmd (macOS) or Ctrl (Windows, Linux) for each action. */
export const SHORTCUT_KEYS: Record<string, ShortcutAction> = {
  s: 'save',
  r: 'refresh',
  // Shows or hides the row detail panel of the Content view.
  i: 'detail',
}

/** The key that goes with Shift and Cmd (macOS) or Shift and Ctrl (Windows, Linux). */
export const SHIFT_SHORTCUT_KEYS: Record<string, ShortcutAction> = {
  e: 'export',
}

/** The key that cancels a running query on its own, with no modifier. */
export const CANCEL_KEY = 'Escape'

/** The key that cancels a running query together with Cmd (macOS) or Ctrl (Windows, Linux). */
export const CANCEL_SHORTCUT_KEY = '.'

/** `navigator.platform` of an Apple device matches this. */
export const MAC_PLATFORM_PATTERN = /mac|iphone|ipad/i
