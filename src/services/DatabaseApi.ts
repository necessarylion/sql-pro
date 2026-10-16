import { invoke } from '@tauri-apps/api/core'
import type {
  AiOptions,
  BrowseRequest,
  ClaudeReply,
  ColumnInfo,
  ColumnSpec,
  ConnectionConfig,
  CsvImport,
  CsvPreview,
  DbObject,
  DbUser,
  Diagram,
  ExportRequest,
  NewColumn,
  NewForeignKey,
  NewIndex,
  GrantableAccess,
  NewUser,
  ResultSet,
  RowUpdate,
  RowsExport,
  SaveResult,
  TableKeys,
  TablePage,
  TableRef,
  UserList,
  Values,
} from '../types'

/**
 * Thin wrappers around the Tauri backend commands.
 * This is the only file that talks to the backend.
 * Every method rejects with a plain string on failure.
 */
export class DatabaseApi {
  connect(id: string, config: ConnectionConfig): Promise<void> {
    return invoke('connect', { id, config })
  }

  /** Opens a new connection `newId` with the same settings as connection `id` uses now. */
  cloneConnection(id: string, newId: string): Promise<void> {
    return invoke('clone_connection', { id, newId })
  }

  disconnect(id: string): Promise<void> {
    return invoke('disconnect', { id })
  }

  useDatabase(id: string, database: string): Promise<void> {
    return invoke('use_database', { id, database })
  }

  listDatabases(id: string): Promise<string[]> {
    return invoke('list_databases', { id })
  }

  listTables(id: string): Promise<TableRef[]> {
    return invoke('list_tables', { id })
  }

  /** Stops the query that runs now on connection `id`. It runs while that query still runs. */
  cancelQuery(id: string): Promise<void> {
    return invoke('cancel_query', { id })
  }

  browse(id: string, request: BrowseRequest): Promise<TablePage> {
    return invoke('browse', { id, request })
  }

  runQuery(id: string, sql: string): Promise<ResultSet[]> {
    return invoke('run_query', { id, sql })
  }

  /**
   * Sends a message to Claude about the database of connection `id`.
   * With no `sessionId`, a new conversation starts with the tables of the database.
   * The backend checks the provider, model and effort in `options` against fixed lists.
   */
  askClaude(
    id: string,
    message: string,
    sessionId: string | null,
    options: AiOptions,
  ): Promise<ClaudeReply> {
    return invoke('ask_claude', { id, message, sessionId, options })
  }

  /** Stops the Claude run of connection `id`. */
  stopClaude(id: string): Promise<void> {
    return invoke('stop_claude', { id })
  }

  /**
   * Saves all changes of a table in one transaction: the deletes, the updates, the inserts.
   * `deletes` holds the primary keys of the rows to delete.
   * When one change fails, nothing is saved.
   */
  saveChanges(
    id: string,
    table: TableRef,
    updates: RowUpdate[],
    inserts: Values[],
    deletes: Values[],
  ): Promise<SaveResult> {
    return invoke('save_changes', { id, table, updates, inserts, deletes })
  }

  addColumn(id: string, table: TableRef, column: ColumnSpec): Promise<void> {
    return invoke('add_column', { id, table, column })
  }

  dropColumn(id: string, table: TableRef, name: string): Promise<void> {
    return invoke('drop_column', { id, table, name })
  }

  /** `name` is the current name of the column. The backend changes only what differs. */
  alterColumn(id: string, table: TableRef, name: string, column: ColumnSpec): Promise<void> {
    return invoke('alter_column', { id, table, name, column })
  }

  /** The indexes and foreign keys of a table. */
  tableKeys(id: string, table: TableRef): Promise<TableKeys> {
    return invoke('table_keys', { id, table })
  }

  addIndex(id: string, table: TableRef, index: NewIndex): Promise<void> {
    return invoke('add_index', { id, table, index })
  }

  dropIndex(id: string, table: TableRef, name: string): Promise<void> {
    return invoke('drop_index', { id, table, name })
  }

  /** Not on SQLite. It cannot change the foreign keys of an existing table. */
  addForeignKey(id: string, table: TableRef, key: NewForeignKey): Promise<void> {
    return invoke('add_foreign_key', { id, table, key })
  }

  /** Not on SQLite. */
  dropForeignKey(id: string, table: TableRef, name: string): Promise<void> {
    return invoke('drop_foreign_key', { id, table, name })
  }

  createTable(id: string, name: string, columns: NewColumn[]): Promise<void> {
    return invoke('create_table', { id, name, columns })
  }

  /** `name` is the new name. The table stays in its schema. */
  renameTable(id: string, table: TableRef, name: string): Promise<void> {
    return invoke('rename_table', { id, table, name })
  }

  dropTable(id: string, table: TableRef): Promise<void> {
    return invoke('drop_table', { id, table })
  }

  /** The triggers and routines of the current database. Views come with `listTables`. */
  listObjects(id: string): Promise<DbObject[]> {
    return invoke('list_objects', { id })
  }

  /** The CREATE statement of a view, trigger or routine. */
  objectDefinition(id: string, object: DbObject): Promise<string> {
    return invoke('object_definition', { id, object })
  }

  /** Drops a trigger or routine. A view is dropped with `dropTable`. */
  dropObject(id: string, object: DbObject): Promise<void> {
    return invoke('drop_object', { id, object })
  }

  truncateTable(id: string, table: TableRef): Promise<void> {
    return invoke('truncate_table', { id, table })
  }

  /** Writes a SQL dump of the current database to the file at `path`. */
  dumpDatabase(id: string, path: string): Promise<void> {
    return invoke('dump_database', { id, path })
  }

  /** Writes tables to a SQL or CSV file. It only reads from the database. */
  exportTables(id: string, request: ExportRequest): Promise<void> {
    return invoke('export_tables', { id, request })
  }

  /** Writes rows the app already has to a SQL or CSV file. It does not touch the database. */
  exportRows(id: string, request: RowsExport): Promise<void> {
    return invoke('export_rows', { id, request })
  }

  listColumns(id: string, table: TableRef): Promise<ColumnInfo[]> {
    return invoke('list_columns', { id, table })
  }

  /** The first rows of a CSV file and how many rows it has. */
  previewCsv(path: string): Promise<CsvPreview> {
    return invoke('preview_csv', { path })
  }

  /** Inserts the rows of a CSV file into a table, in one transaction. Gives the row count. */
  importCsv(id: string, request: CsvImport): Promise<number> {
    return invoke('import_csv', { id, request })
  }

  /** Runs all statements of a SQL file, in one transaction. Gives the statement count. */
  importSql(id: string, path: string): Promise<number> {
    return invoke('import_sql', { id, path })
  }

  /** The tables of the current database with their columns and foreign keys. */
  diagram(id: string): Promise<Diagram> {
    return invoke('diagram', { id })
  }

  /** Writes the bytes of a PNG image to the file at `path`. */
  saveImage(path: string, data: number[]): Promise<void> {
    return invoke('save_image', { path, data })
  }

  readTextFile(path: string): Promise<string> {
    return invoke('read_text_file', { path })
  }

  /** An older file at `path` is replaced. */
  writeTextFile(path: string, text: string): Promise<void> {
    return invoke('write_text_file', { path, text })
  }

  listUsers(id: string): Promise<UserList> {
    return invoke('list_users', { id })
  }

  createUser(id: string, user: NewUser): Promise<void> {
    return invoke('create_user', { id, user })
  }

  dropUser(id: string, user: DbUser): Promise<void> {
    return invoke('drop_user', { id, name: user.name, host: user.host })
  }

  /** Sets what the user may do on the current database. */
  setUserAccess(id: string, user: DbUser, access: GrantableAccess): Promise<void> {
    return invoke('set_user_access', { id, name: user.name, host: user.host, access })
  }

  setUserPassword(id: string, user: DbUser, password: string): Promise<void> {
    return invoke('set_user_password', { id, name: user.name, host: user.host, password })
  }

  savePassword(id: string, password: string): Promise<void> {
    return invoke('save_password', { id, password })
  }

  loadPassword(id: string): Promise<string> {
    return invoke('load_password', { id })
  }

  deletePassword(id: string): Promise<void> {
    return invoke('delete_password', { id })
  }
}
