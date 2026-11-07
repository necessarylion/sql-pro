export type ShortcutAction = 'save' | 'refresh' | 'export' | 'detail' | 'duplicate' | 'settings'

/** The key that goes with Cmd (macOS) or Ctrl (Windows, Linux) for each action. */
export const SHORTCUT_KEYS: Record<string, ShortcutAction> = {
  s: 'save',
  r: 'refresh',
  // Shows or hides the row detail panel of the Content view.
  i: 'detail',
  // Duplicates the selected rows of the Content view as new rows.
  d: 'duplicate',
  // Opens the Settings window. On macOS the app menu has the same key.
  ',': 'settings',
}

/** The key that goes with Shift and Cmd (macOS) or Shift and Ctrl (Windows, Linux). */
export const SHIFT_SHORTCUT_KEYS: Record<string, ShortcutAction> = {
  e: 'export',
}

/** The key that cancels a running query on its own, with no modifier. */
export const CANCEL_KEY = 'Escape'

/**
 * Places where Escape has its own job: an open dialog, a field, the SQL editor, a menu.
 * Escape there never stops a backup.
 */
export const OWN_ESCAPE_SELECTOR =
  'dialog[open], input, textarea, select, [contenteditable], .cm-editor, .menu-backdrop'

/** The key that cancels a running query together with Cmd (macOS) or Ctrl (Windows, Linux). */
export const CANCEL_SHORTCUT_KEY = '.'

/** The key that selects all rows of a grid together with Cmd (macOS) or Ctrl (Windows, Linux). */
export const SELECT_ALL_KEY = 'a'

/** `navigator.platform` of an Apple device matches this. */
export const MAC_PLATFORM_PATTERN = /mac|iphone|ipad/i

/** The key that copies the selected rows of a grid together with Cmd (macOS) or Ctrl (Windows, Linux). */
export const COPY_KEY = 'c'
