/** The folder list is saved under this key. The saved connections have their own key. */
export const FOLDERS_STORAGE_KEY = 'sql-pro.folders'

/**
 * The colours a folder can have. A folder saves the name, not the hex value.
 * The hex values are the `--folder-*` tokens in style.css, one set for each theme.
 */
export const FOLDER_COLORS = ['grey', 'red', 'orange', 'green', 'blue', 'purple'] as const

export type FolderColor = (typeof FOLDER_COLORS)[number]

/** The icons a folder can have. FolderIcon.vue maps each name to its picture. */
export const FOLDER_ICONS = [
  'folder',
  'server',
  'database',
  'cloud',
  'briefcase',
  'home',
  'rocket',
  'flask',
  'shield',
  'star',
  'globe',
  'box',
] as const

export type FolderIconName = (typeof FOLDER_ICONS)[number]

/** A new folder gets these. A saved value that is not known becomes these too. */
export const DEFAULT_FOLDER_COLOR: FolderColor = 'grey'
export const DEFAULT_FOLDER_ICON: FolderIconName = 'folder'

export const FOLDER_NAME_MAX_LENGTH = 40

/** A connection row must move this many pixels before the drag starts. A click moves less. */
export const DRAG_START_DISTANCE = 4
