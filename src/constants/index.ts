import type { ConnectionConfig, DbObjectKind, Driver, SessionView, SshConfig } from '../types'

/** The word for each kind of database object, as the UI shows it. */
export const OBJECT_KIND_LABELS: Record<DbObjectKind, string> = {
  view: 'View',
  trigger: 'Trigger',
  procedure: 'Procedure',
  function: 'Function',
}

/** Rows loaded per page in the content view. */
export const PAGE_SIZE = 300

/** The status of a tab after the user cancelled its query. */
export const QUERY_CANCELLED_STATUS = 'Query cancelled'

/** The table and file name for exported query result rows, which have no table. */
export const QUERY_RESULT_NAME = 'query_result'

/** Longer cell text is cut in the grid. The full value is still used for editing. */
export const CELL_PREVIEW_LENGTH = 200

/** In the row detail panel, a longer value (or one with a line break) gets a multi-line box. */
export const DETAIL_LINE_LENGTH = 80

/** The spaces of one indent level when the row detail panel formats JSON. */
export const JSON_INDENT = 2

/** The type of the `id` column that a new table starts with. All three databases know it. */
export const ID_COLUMN_TYPE = 'INTEGER'

export const FAVORITES_STORAGE_KEY = 'sql-pro.favorites'

export const DRIVER_LABELS: Record<Driver, string> = {
  mysql: 'MySQL',
  postgres: 'PostgreSQL',
  sqlite: 'SQLite',
}

/** The short text in the small coloured badge of a driver. */
export const DRIVER_BADGES: Record<Driver, string> = {
  mysql: 'MY',
  postgres: 'PG',
  sqlite: 'SL',
}

export const DEFAULT_CONFIGS: Record<Driver, ConnectionConfig> = {
  mysql: {
    driver: 'mysql',
    host: '127.0.0.1',
    port: 3306,
    user: 'root',
    password: '',
    database: '',
    ssh: null,
  },
  postgres: {
    driver: 'postgres',
    host: '127.0.0.1',
    port: 5432,
    user: 'postgres',
    password: '',
    database: 'postgres',
    ssh: null,
  },
  sqlite: { driver: 'sqlite', host: '', port: 0, user: '', password: '', database: '', ssh: null },
}

/** The scheme of a connection string -> the driver. */
export const CONNECTION_STRING_DRIVERS: Record<string, Driver> = {
  mysql: 'mysql',
  mariadb: 'mysql',
  postgres: 'postgres',
  postgresql: 'postgres',
  sqlite: 'sqlite',
  file: 'sqlite',
}

export const CONNECTION_STRING_EXAMPLE = 'postgres://user:password@host:5432/database'

/** It must never hold the text the user typed: that text can hold a password. */
export const CONNECTION_STRING_ERROR = `This is not a connection string. Example: ${CONNECTION_STRING_EXAMPLE}`

export const DEFAULT_SSH_CONFIG: SshConfig = {
  host: '',
  port: 22,
  user: '',
  password: '',
  keyPath: '',
}

/** Added to a favorite id to get the keychain entry of its SSH password. */
export const SSH_PASSWORD_SUFFIX = ':ssh'

/** Added to the name of a saved connection to name its copy. */
export const FAVORITE_COPY_SUFFIX = ' copy'

/** The order here is the order of the buttons in the toolbar. */
export const VIEW_LABELS: Record<SessionView, string> = {
  structure: 'Structure',
  content: 'Content',
  query: 'Query',
  users: 'Users',
  diagram: 'Diagram',
}

/** Binary values arrive as hex text. Editing them would corrupt them. */
export const BINARY_TYPE_PATTERN = /blob|binary|bytea/i

/** Columns of these types hold numbers. The grid aligns them to the right. */
export const NUMERIC_TYPE_PATTERN = /int|decimal|numeric|float|double|real|serial|money/i

/** The height of one grid row, in pixels. The CSS of DataGrid uses the same number. */
export const GRID_ROW_HEIGHT = 22

/**
 * The grid draws only the rows that can be seen, plus this many rows above and below.
 * The drawn part moves in steps of this many rows, so a small scroll draws nothing new.
 */
export const GRID_ROW_BUFFER = 20

/** A column of the grid cannot be dragged narrower than this, in pixels. */
export const COLUMN_MIN_WIDTH = 40

/** The title of the question that is asked before unsaved changes are lost. */
export const UNSAVED_TITLE = 'Unsaved changes'

/** The title of the question that is asked before a save deletes rows. */
export const DELETE_ROWS_TITLE = 'Delete rows'
