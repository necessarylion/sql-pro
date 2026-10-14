/** The zoom levels in percent, like the steps of a browser. The ends are the limits. */
export const ZOOM_STEPS = [50, 67, 75, 80, 90, 100, 110, 125, 150, 175, 200]
export const ZOOM_DEFAULT = 100

export const ZOOM_STORAGE_KEY = 'sql-pro.zoom'

/** The CSS custom property on the root element that holds the zoom factor. See style.css. */
export const ZOOM_PROPERTY = '--zoom'

export type ZoomAction = 'in' | 'out' | 'reset'

/** The typed character that goes with Cmd or Ctrl. `+` is `=` with Shift, `_` is `-` with Shift. */
export const ZOOM_KEYS: Record<string, ZoomAction> = {
  '=': 'in',
  '+': 'in',
  '-': 'out',
  _: 'out',
  '0': 'reset',
}

/**
 * The physical keys, for when the character does not match.
 * The numpad gives no digit with NumLock off. Some layouts need Shift for the digit 0.
 */
export const ZOOM_CODES: Record<string, ZoomAction> = {
  NumpadAdd: 'in',
  NumpadSubtract: 'out',
  Numpad0: 'reset',
  Digit0: 'reset',
}
