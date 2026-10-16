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

/** The height of the result grid in the Ask AI view, in pixels. One height for all tabs. */
export const AI_RESULT_DEFAULT_HEIGHT = 200
export const AI_RESULT_MIN_HEIGHT = 80
// Only stops a wrong saved value. The CSS keeps the chat log at least 120px tall. See AskAiView.vue.
export const AI_RESULT_MAX_HEIGHT = 2000

export const AI_RESULT_HEIGHT_STORAGE_KEY = 'sql-pro.ai-result-height'
export const AI_RESULT_HEIGHT_PROPERTY = '--ai-result-height'

/** The sizes the user can drag. Each name is also a field of LayoutService. */
export type LayoutSize = 'sidebarWidth' | 'editorHeight' | 'detailWidth' | 'aiResultHeight'

/** The sizes whose drag handle lies on the left edge of the pane. The pane grows to the left. */
export const LEFT_EDGE_SIZES: LayoutSize[] = ['detailWidth']

/** The sizes that are heights. Their handle is a horizontal line. */
export const HEIGHT_SIZES: LayoutSize[] = ['editorHeight', 'aiResultHeight']

/** The heights whose drag handle lies on the top edge of the pane. The pane grows upwards. */
export const TOP_EDGE_SIZES: LayoutSize[] = ['aiResultHeight']

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
  aiResultHeight: {
    fallback: AI_RESULT_DEFAULT_HEIGHT,
    min: AI_RESULT_MIN_HEIGHT,
    max: AI_RESULT_MAX_HEIGHT,
    storageKey: AI_RESULT_HEIGHT_STORAGE_KEY,
    property: AI_RESULT_HEIGHT_PROPERTY,
  },
}

/** The sections of the Structure view that the user can open and close. Each name is also a field of LayoutService. */
export type KeysSection = 'columnsOpen' | 'indexesOpen' | 'foreignKeysOpen'

/** Where each section saves if it is open. All start open. */
export const KEYS_SECTION_STORAGE_KEYS: Record<KeysSection, string> = {
  columnsOpen: 'sql-pro.columns-open',
  indexesOpen: 'sql-pro.indexes-open',
  foreignKeysOpen: 'sql-pro.foreign-keys-open',
}

/** The heights of the Structure view sections the user can drag. Each name is also a field of LayoutService. */
export type SectionHeight = 'columnsHeight' | 'indexesHeight' | 'foreignKeysHeight'

/** Where each section saves its height. No saved height means the section is as tall as its content. */
export const SECTION_HEIGHT_STORAGE_KEYS: Record<SectionHeight, string> = {
  columnsHeight: 'sql-pro.columns-height',
  indexesHeight: 'sql-pro.indexes-height',
  foreignKeysHeight: 'sql-pro.foreign-keys-height',
}

/** The smallest section: heading (30) + table header (28) + one row (23). */
export const SECTION_MIN_HEIGHT = 81
// Only stops a wrong saved value. The drag keeps a section inside the window.
export const SECTION_MAX_HEIGHT = 4000
