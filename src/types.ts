// Shared types. They match the Tauri backend commands.

export type Driver = 'mysql' | 'postgres' | 'sqlite'

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

/** `schema` is set only for postgres. */
export interface TableRef {
  schema: string | null
  name: string
  kind: 'table' | 'view'
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

export type SessionView = 'structure' | 'content' | 'query' | 'users' | 'diagram'

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
  /** The schema of `table`. Set only for postgres. */
  schema: string | null
  table: string
  columns: string[]
  refSchema: string | null
  refTable: string
  refColumns: string[]
  /** True when the key columns are exactly a primary key or a unique index of `table`. */
  unique: boolean
}

/** All tables of the current database and their foreign keys. Views are left out. */
export interface Diagram {
  tables: DiagramTable[]
  relations: Relation[]
}
