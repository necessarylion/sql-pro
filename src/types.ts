// Shared types. They match the Tauri backend commands.

export type Driver = 'mysql' | 'postgres' | 'mssql' | 'sqlite'

/** For sqlite, `database` is the file path. The other fields are empty or 0. */
export interface ConnectionConfig {
  driver: Driver
  host: string
  port: number
  user: string
  password: string
  database: string
  /** Set to reach the database through an SSH server. Not used for sqlite. */
  ssh: SshConfig | null
}

/** `password` is the login password or the key passphrase. Empty = keys and agent only. */
export interface SshConfig {
  host: string
  port: number
  user: string
  password: string
  keyPath: string
}

/** `schema` is set only for postgres and mssql. */
export interface TableRef {
  schema: string | null
  name: string
  kind: 'table' | 'view'
}

export type DbObjectKind = 'view' | 'trigger' | 'procedure' | 'function'

/** A view, trigger, procedure or function. Only PostgreSQL fills `schema` and `args`. */
export interface DbObject {
  kind: DbObjectKind
  schema: string | null
  name: string
  /** The table of a trigger. */
  table: string | null
  /** The argument types of a PostgreSQL routine. Two routines can share a name. */
  args: string | null
}

/** The SQL definition that is shown in the place of a table. */
export interface ObjectDefinition {
  object: DbObject
  sql: string
}

export interface ColumnInfo {
  name: string
  dataType: string
  nullable: boolean
  default: string | null
  primaryKey: boolean
  extra: string
}

/** The backend sends every value as text. */
export type Row = (string | null)[]

export interface ResultSet {
  columns: string[]
  rows: Row[]
  affected: number
  truncated: boolean
}

export interface TablePage {
  columns: ColumnInfo[]
  rows: Row[]
  total: number
}

/** What the user types to add or change a column. `dataType` and `default` are raw SQL. */
export interface ColumnSpec {
  name: string
  dataType: string
  nullable: boolean
  /** null = no default. */
  default: string | null
}

/** One column of a table that is created. */
export interface NewColumn extends ColumnSpec {
  primaryKey: boolean
  /** The database numbers the rows itself. Only for a column that is the whole primary key. */
  autoIncrement: boolean
}

export type FilterOperator =
  | 'eq'
  | 'neq'
  | 'lt'
  | 'gt'
  | 'lte'
  | 'gte'
  | 'contains'
  | 'notContains'
  | 'startsWith'
  | 'endsWith'
  | 'in'
  | 'isNull'
  | 'isNotNull'

/** One filter rule, as the backend wants it. */
export interface FilterRule {
  column: string
  operator: FilterOperator
  value: string
}

/** One row of the filter area. `column` can be RAW_FILTER_COLUMN: then `value` is raw SQL. */
export interface FilterRow extends FilterRule {
  enabled: boolean
}

/**
 * `filter` is a raw SQL WHERE body. `orderBy` is a column name. Both can be ''.
 * The backend joins `filter` and all `filters` with AND.
 */
export interface BrowseRequest {
  table: TableRef
  filter: string
  filters: FilterRule[]
  orderBy: string
  descending: boolean
  limit: number
  offset: number
  /** The total we already know. The backend then skips the count. Only for a page turn. */
  total?: number
}

/** Column name -> value. */
export type Values = Record<string, string | null>

/** The pending edits of one row. `key` holds the primary key values as they were loaded. */
export interface RowUpdate {
  key: Values
  changes: Values
}

export interface SaveResult {
  updated: number
  inserted: number
  deleted: number
}

export type SessionView = 'structure' | 'content' | 'query' | 'ask' | 'users' | 'diagram'

/** One reply of Claude Code in the Ask AI view. */
export interface ClaudeReply {
  /** Send it with the next message to go on in the same conversation. */
  sessionId: string
  message: string
  /** One SQL statement Claude wants to run. null when Claude is done. */
  sql: string | null
  /** True when `sql` surely only reads. Else the user must confirm it first. */
  isRead: boolean
}

/** What Ask AI does with SQL that can change data: ask first, or never run it. */
export type AiMode = 'ask' | 'readOnly'

/** The choices sent with each message. null = the tool's own default. */
export interface AiOptions {
  provider: string
  model: string | null
  effort: string | null
  readOnly: boolean
}

/** One saved Ask AI chat. The result rows are never saved: they can hold private data. */
export interface SavedChat {
  id: string
  /** The first question, cut short. */
  title: string
  provider: string
  /** Which server the chat is about. See `AiStore.connectionKey`. */
  connection: string
  database: string
  /** The session of the AI tool, for `--resume`. */
  sessionId: string | null
  /** Times in milliseconds since 1970. */
  created: number
  updated: number
  entries: ChatEntry[]
}

/** One line of the Ask AI chat. */
export interface ChatEntry {
  kind: 'user' | 'ai' | 'sql' | 'note'
  text: string
  /** For `sql`: what happened, like "3 rows" or "Rejected". */
  outcome?: string
  /** For `sql`: true when the outcome is an error or a rejection. For `note`: an error. */
  failed?: boolean
  /** For `sql`: a write that waits for the user. The chat shows an approval card. */
  pending?: boolean
  /** For `user`: the attached files. Only names and types, never the content. */
  files?: ChatFile[]
}

/**
 * A file for the AI: a PNG, JPEG or WebP image, or a CSV file. The backend checks it.
 * `data` is base64 for an image and the text for a CSV file.
 */
export interface Attachment {
  name: string
  mediaType: string
  data: string
  /** The lines of a CSV file, with the header. 0 for an image. */
  rows: number
}

/** An attached file as the chat shows and saves it. */
export interface ChatFile {
  name: string
  mediaType: string
  rows: number
  /** False in a chat from the history: the content was not saved. */
  kept: boolean
}

/**
 * What a user may do on the current database.
 * `admin` = all rights. `custom` = some other mix of rights.
 */
export type AccessLevel = 'none' | 'read' | 'write' | 'admin' | 'custom'

/** The levels that can be given to a user here. */
export type GrantableAccess = 'none' | 'read' | 'write'

export interface DbUser {
  name: string
  /** The account host of MySQL. null for PostgreSQL. */
  host: string | null
  access: AccessLevel
  /** True for the account this connection is logged in as. */
  current: boolean
}

/** `canManage` is false when the logged-in account may not manage users. */
export interface UserList {
  canManage: boolean
  users: DbUser[]
}

export interface NewUser {
  name: string
  host: string | null
  password: string
  access: GrantableAccess
}

/** One column of the DataGrid component. */
export interface GridColumn {
  name: string
  type?: string
  nullable?: boolean
  /** Cells of this column cannot be edited, even in an editable grid. */
  readOnly?: boolean
}

/** One table box of the diagram. */
export interface DiagramTable {
  table: TableRef
  columns: ColumnInfo[]
}

/** One foreign key. `table` holds the key columns. `refTable` is the table they point at. */
export interface Relation {
  name: string
  /** The schema of `table`. Set only for postgres and mssql. */
  schema: string | null
  table: string
  columns: string[]
  refSchema: string | null
  refTable: string
  refColumns: string[]
  /** True when the key columns are exactly a primary key or a unique index of `table`. */
  unique: boolean
}

/** One index of a table. `name` is empty for the primary key of a SQLite row id table. */
export interface IndexInfo {
  name: string
  /** In index order. An empty text is a part that is an expression. */
  columns: string[]
  unique: boolean
  primary: boolean
}

/** What a foreign key does when the referenced row is deleted or its key changes. */
export type ReferentialAction = 'NO ACTION' | 'RESTRICT' | 'CASCADE' | 'SET NULL' | 'SET DEFAULT'

/** One foreign key of the open table. `name` is empty on SQLite. */
export interface ForeignKey {
  name: string
  columns: string[]
  /** Set only when the referenced table is in another schema or database. */
  refSchema: string | null
  refTable: string
  /** In the same order as `columns`. */
  refColumns: string[]
  /** As the database writes it, like CASCADE. */
  onDelete: string
  onUpdate: string
}

/** The indexes and foreign keys of one table. */
export interface TableKeys {
  indexes: IndexInfo[]
  foreignKeys: ForeignKey[]
}

export interface NewIndex {
  name: string
  columns: string[]
  unique: boolean
}

export interface NewForeignKey {
  name: string
  columns: string[]
  refTable: TableRef
  /** In the same order as `columns`. */
  refColumns: string[]
  onDelete: ReferentialAction
  onUpdate: ReferentialAction
}

/** All tables of the current database and their foreign keys. Views are left out. */
export interface Diagram {
  tables: DiagramTable[]
  relations: Relation[]
}

export type ExportFormat = 'sql' | 'csv'

/** What to export, and where to write it. */
export interface ExportRequest {
  tables: TableRef[]
  format: ExportFormat
  /** SQL only: write the CREATE statement of each table. */
  structure: boolean
  /** SQL only: write the rows as INSERT statements. A CSV file always has the rows. */
  data: boolean
  /** The columns to export, in this order. Empty means all. Only for one table. */
  columns: string[]
  /** The file to write. For CSV it may be a folder: then each table goes to `<table>.csv`. */
  path: string
}

/** Rows the app already shows, to write to a SQL or CSV file. */
export interface RowsExport {
  format: ExportFormat
  /** SQL only: the table that the INSERT statements name. */
  table: TableRef
  columns: string[]
  rows: Row[]
  path: string
}

/** The start of a CSV file, for the import dialog. */
export interface CsvPreview {
  /** The first rows, as they are in the file. The header row is one of them. */
  rows: string[][]
  /** All rows of the file, the header row too. */
  count: number
}

/** One table column that gets its value from CSV field number `field` (from 0). */
export interface CsvColumn {
  name: string
  field: number
}

/** What to import from a CSV file, and into which table. */
export interface CsvImport {
  table: TableRef
  path: string
  /** The first row holds the column names. It is not imported. */
  header: boolean
  /** An empty field becomes NULL. Else it becomes an empty text. */
  emptyIsNull: boolean
  /** The columns to fill. The others get their default. */
  columns: CsvColumn[]
}
