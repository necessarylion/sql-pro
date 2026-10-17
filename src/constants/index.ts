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

/** The type of the `id` column that a new table starts with. All databases know it. */
export const ID_COLUMN_TYPE = 'INTEGER'

export const FAVORITES_STORAGE_KEY = 'sql-pro.favorites'

export const DRIVER_LABELS: Record<Driver, string> = {
  mysql: 'MySQL',
  postgres: 'PostgreSQL',
  mssql: 'SQL Server',
  sqlite: 'SQLite',
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
  mssql: {
    driver: 'mssql',
    host: '127.0.0.1',
    port: 1433,
    user: 'sa',
    password: '',
    database: 'master',
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
  mssql: 'mssql',
  sqlserver: 'mssql',
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
  ask: 'Ask AI',
  users: 'Users',
  processes: 'Processes',
  diagram: 'Diagram',
}

/** Binary values arrive as hex text. Editing them would corrupt them. */
export const BINARY_TYPE_PATTERN = /blob|binary|bytea/i

/** Date and time types with a time zone. The native picker cannot keep the zone, so they get no picker. */
export const ZONED_TIME_TYPE_PATTERN = /with time zone|timestamptz|timetz|datetimeoffset/i

/** SQL Server `timestamp` is a binary row version (`rowversion`), not a date. */
export const ROW_VERSION_TYPE_PATTERN = /^(timestamp|rowversion)\b/i

/** Column types that get a date picker in the row detail panel. */
export const DATE_TYPE_PATTERN = /^date$/i
export const TIME_TYPE_PATTERN = /^time\b/i
export const DATETIME_TYPE_PATTERN = /^((small)?datetime2?|timestamp)\b/i

/** Columns of these types hold numbers. The grid aligns them to the right. */
export const NUMERIC_TYPE_PATTERN = /int|decimal|numeric|float|double|real|serial|money/i

/** The height of one grid row, in pixels. The CSS of DataGrid uses the same number. */
export const GRID_ROW_HEIGHT = 23

/**
 * The grid draws only the rows that can be seen, plus this many rows above and below.
 * The drawn part moves in steps of this many rows, so a small scroll draws nothing new.
 */
export const GRID_ROW_BUFFER = 20

/** A column of the grid cannot be dragged narrower than this, in pixels. */
export const COLUMN_MIN_WIDTH = 40
/** How far, in pixels, a header press must go before it moves the column. A shorter one is a click. */
export const COLUMN_MOVE_THRESHOLD = 4

/** The title of the question that is asked before unsaved changes are lost. */
export const UNSAVED_TITLE = 'Unsaved changes'

/** The title of the question that is asked before a save deletes rows. */
export const DELETE_ROWS_TITLE = 'Delete rows'

/**
 * The default schemas: `public` in PostgreSQL and `dbo` in SQL Server.
 * The UI hides these in front of a table name.
 */
// ponytail: a user's own default schema can differ. Read it from the server if that matters.
export const DEFAULT_SCHEMAS: readonly string[] = ['public', 'dbo']
