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

/** The sizes the user can drag. Each name is also a field of LayoutService. */
export type LayoutSize = 'sidebarWidth' | 'editorHeight'

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
}
