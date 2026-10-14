/** The width of the left sidebar, in pixels. It is one width for all tabs and both screens. */
export const SIDEBAR_DEFAULT_WIDTH = 210
export const SIDEBAR_MIN_WIDTH = 160
export const SIDEBAR_MAX_WIDTH = 480

export const SIDEBAR_WIDTH_STORAGE_KEY = 'sql-pro.sidebar-width'

/** The CSS custom property on the root element that holds the width. See style.css. */
export const SIDEBAR_WIDTH_PROPERTY = '--sidebar-width'

/** The height of the SQL editor, in pixels. It is one height for all tabs. */
export const EDITOR_DEFAULT_HEIGHT = 220
export const EDITOR_MIN_HEIGHT = 80
// Only stops a wrong saved value. The CSS keeps the editor inside the window. See QueryView.vue.
export const EDITOR_MAX_HEIGHT = 2000

export const EDITOR_HEIGHT_STORAGE_KEY = 'sql-pro.editor-height'
export const EDITOR_HEIGHT_PROPERTY = '--editor-height'

/** The width of the row detail panel on the right of the Content view. One width for all tabs. */
export const DETAIL_DEFAULT_WIDTH = 320
export const DETAIL_MIN_WIDTH = 200
// Only stops a wrong saved value. The CSS keeps the grid visible. See ContentView.vue.
export const DETAIL_MAX_WIDTH = 2000

export const DETAIL_WIDTH_STORAGE_KEY = 'sql-pro.detail-width'
export const DETAIL_WIDTH_PROPERTY = '--detail-width'

/** The height of the indexes and foreign keys area in the Structure view. One height for all tabs. */
export const KEYS_DEFAULT_HEIGHT = 180
export const KEYS_MIN_HEIGHT = 60
// Only stops a wrong saved value. The CSS keeps the column grid visible. See StructureView.vue.
export const KEYS_MAX_HEIGHT = 2000

export const KEYS_HEIGHT_STORAGE_KEY = 'sql-pro.keys-height'
export const KEYS_HEIGHT_PROPERTY = '--keys-height'

/** Holds '1' when the row detail panel is open. */
export const DETAIL_OPEN_STORAGE_KEY = 'sql-pro.detail-open'

/** The sizes the user can drag. Each name is also a field of LayoutService. */
export type LayoutSize = 'sidebarWidth' | 'editorHeight' | 'detailWidth' | 'keysHeight'

/** The sizes whose drag handle lies on the left edge of the pane. The pane grows to the left. */
export const LEFT_EDGE_SIZES: LayoutSize[] = ['detailWidth']

/** The sizes whose drag handle lies on the top edge of the pane. The pane grows upward. */
export const TOP_EDGE_SIZES: LayoutSize[] = ['keysHeight']

/** The sizes that are heights. Their handle is a horizontal line. */
export const HEIGHT_SIZES: LayoutSize[] = ['editorHeight', 'keysHeight']

export interface LayoutSizeRule {
  fallback: number
  min: number
  max: number
  storageKey: string
  property: string
}

export const LAYOUT_SIZES: Record<LayoutSize, LayoutSizeRule> = {
  sidebarWidth: {
    fallback: SIDEBAR_DEFAULT_WIDTH,
    min: SIDEBAR_MIN_WIDTH,
    max: SIDEBAR_MAX_WIDTH,
    storageKey: SIDEBAR_WIDTH_STORAGE_KEY,
    property: SIDEBAR_WIDTH_PROPERTY,
  },
  editorHeight: {
    fallback: EDITOR_DEFAULT_HEIGHT,
    min: EDITOR_MIN_HEIGHT,
    max: EDITOR_MAX_HEIGHT,
    storageKey: EDITOR_HEIGHT_STORAGE_KEY,
    property: EDITOR_HEIGHT_PROPERTY,
  },
  detailWidth: {
    fallback: DETAIL_DEFAULT_WIDTH,
    min: DETAIL_MIN_WIDTH,
    max: DETAIL_MAX_WIDTH,
    storageKey: DETAIL_WIDTH_STORAGE_KEY,
    property: DETAIL_WIDTH_PROPERTY,
  },
  keysHeight: {
    fallback: KEYS_DEFAULT_HEIGHT,
    min: KEYS_MIN_HEIGHT,
    max: KEYS_MAX_HEIGHT,
    storageKey: KEYS_HEIGHT_STORAGE_KEY,
    property: KEYS_HEIGHT_PROPERTY,
  },
}
