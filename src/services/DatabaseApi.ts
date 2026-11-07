import { invoke } from '@tauri-apps/api/core'
import { emit, listen, type Event, type UnlistenFn } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import type {
  AiOptions,
  Attachment,
  BrowseRequest,
  ClaudeReply,
  Notes,
  ColumnInfo,
  ColumnSpec,
  ConnectionConfig,
  CsvImport,
  CsvPreview,
  DbObject,
  DbProcess,
  QueryStat,
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
  RowsSql,
  SaveResult,
  SchemaChange,
  SyncSql,
  TableKeys,
  TablePage,
  TableRef,
  UserList,
  Values,
} from '../types'
import { OPEN_FILES_EVENT } from '../constants/files'
import { SETTINGS_CHANGED_EVENT, type SettingsSnapshot } from '../constants/settings'

/**
 * Thin wrappers around the Tauri backend commands.
 * This is the only file that talks to the backend.
 * Every method rejects with a plain string on failure.
 */
export class DatabaseApi {
  connect(id: string, config: ConnectionConfig): Promise<void> {
    return invoke('connect', { id, config })
  }

  /** Stops the `connect` that still runs for `id`. That connect then rejects. */
  cancelConnect(id: string): Promise<void> {
    return invoke('cancel_connect', { id })
  }

  /** Opens a connection with `config`, checks it and closes it again. Nothing is kept. */
  testConnection(config: ConnectionConfig): Promise<void> {
    return invoke('test_connection', { config })
  }

  /** Opens a new connection `newId` with the same settings as connection `id` uses now. */
  cloneConnection(id: string, newId: string): Promise<void> {
    return invoke('clone_connection', { id, newId })
  }

  /** The font families installed on the machine, sorted. */
  listFonts(): Promise<string[]> {
    return invoke('list_fonts')
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

  /** Creates a database. `charset` and `collation` are only for MySQL. Empty means the default. */
  createDatabase(id: string, name: string, charset: string, collation: string): Promise<void> {
    return invoke('create_database', { id, name, charset, collation })
  }

  /** Drops a database. The backend refuses the one the tab uses now. */
  dropDatabase(id: string, name: string): Promise<void> {
    return invoke('drop_database', { id, name })
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

  /** The query plan of one statement. The backend never runs the statement itself. */
  explain(id: string, sql: string): Promise<ResultSet[]> {
    return invoke('explain', { id, sql })
  }

  /**
   * Sends a message to Claude about the database of connection `id`.
   * With no `sessionId`, a new conversation starts with the tables of the database.
   * The backend checks the provider, model and effort in `options` against fixed lists.
   * The backend puts the notes of the database (`claude.md`) in front of a new conversation,
   * and also when `includeNotes` is true (the user changed them during the chat).
   */
  askClaude(
    id: string,
    message: string,
    sessionId: string | null,
    options: AiOptions,
    attachments: Attachment[] = [],
    includeNotes = false,
  ): Promise<ClaudeReply> {
    return invoke('ask_claude', { id, message, sessionId, options, attachments, includeNotes })
  }

  /** The notes for the AI about the current database of connection `id`. */
  readNotes(id: string): Promise<Notes> {
    return invoke('read_notes', { id })
  }

  /** Replaces the notes for the AI about the current database of connection `id`. */
  writeNotes(id: string, text: string): Promise<void> {
    return invoke('write_notes', { id, text })
  }

  /**
   * Runs a read of the AI in a guard that blocks writes and keeps nothing.
   * It refuses to run while the tab has an open transaction.
   */
  runAiRead(id: string, sql: string): Promise<ResultSet[]> {
    return invoke('run_ai_read', { id, sql })
  }

  /** True when SQL only reads. The same check as for the SQL of the AI. */
  isReadSql(sql: string): Promise<boolean> {
    return invoke('is_read_sql', { sql })
  }

  /** Reads a file for the AI. The backend checks its type and size. */
  readAttachment(path: string): Promise<Attachment> {
    return invoke('read_attachment', { path })
  }

  /** The rows of CSV text, read like the CSV import reads them. */
  parseCsv(text: string): Promise<string[][]> {
    return invoke('parse_csv', { text })
  }

  /** Checks an image pasted from the clipboard. `data` is base64. */
  attachmentFromBase64(name: string, data: string): Promise<Attachment> {
    return invoke('attachment_from_base64', { name, data })
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

  /**
   * Runs `sql`, the CREATE statement of a view, trigger or routine. With `object` it replaces
   * that object. The backend picks the right way for each database. On an error the old one stays.
   */
  saveObject(id: string, object: DbObject | null, sql: string): Promise<void> {
    return invoke('save_object', { id, object, sql })
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

  /** One INSERT statement per row, quoted for the database of the tab. It does not touch the database. */
  rowsInsertSql(id: string, request: RowsSql): Promise<string> {
    return invoke('rows_insert_sql', { id, request })
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

  /** Gives or takes away the right to read the server query stats. */
  setUserQueryStats(id: string, user: DbUser, on: boolean): Promise<void> {
    return invoke('set_user_query_stats', { id, name: user.name, host: user.host, on })
  }

  setUserPassword(id: string, user: DbUser, password: string): Promise<void> {
    return invoke('set_user_password', { id, name: user.name, host: user.host, password })
  }

  listProcesses(id: string): Promise<DbProcess[]> {
    return invoke('list_processes', { id })
  }

  /** `queryOnly` stops only the running statement. Else the whole connection ends. */
  killProcess(id: string, process: number, queryOnly: boolean): Promise<void> {
    return invoke('kill_process', { id, process, queryOnly })
  }

  /** The query shapes of the server from the last `days` days. PostgreSQL does not use `days`. */
  queryStats(id: string, days: number): Promise<QueryStat[]> {
    return invoke('query_stats', { id, days })
  }

  /** What must change in the database of tab `target` to make it like tab `source`. */
  compareSchemas(source: string, target: string): Promise<SchemaChange[]> {
    return invoke('compare_schemas', { source, target })
  }

  /** The SQL for the picked differences. It only builds the SQL. Nothing runs. */
  schemaSyncSql(source: string, target: string, picked: string[]): Promise<SyncSql> {
    return invoke('schema_sync_sql', { source, target, picked })
  }

  /**
   * Builds the sync SQL again on the backend and runs it on tab `target`, in one transaction
   * where the database can. It refuses when the new SQL is not `reviewed`, the SQL the user saw.
   */
  runSchemaSync(
    source: string,
    target: string,
    picked: string[],
    reviewed: string[],
  ): Promise<void> {
    return invoke('run_schema_sync', { source, target, picked, reviewed })
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

  /** The files the OS asked the app to open. Each path comes only once. */
  takeOpenedFiles(): Promise<string[]> {
    return invoke('take_opened_files')
  }

  /** Calls `handler` when the OS gives the app more files to open. Call the result to stop. */
  onOpenedFiles(handler: () => void): Promise<UnlistenFn> {
    return listen(OPEN_FILES_EVENT, handler)
  }

  /** Opens the Settings window, or brings it to the front when it is open. */
  openSettingsWindow(): Promise<void> {
    return invoke('open_settings_window')
  }

  /** Sends the settings to every window. */
  emitSettings(settings: SettingsSnapshot): Promise<void> {
    return emit(SETTINGS_CHANGED_EVENT, settings)
  }

  /** Calls `handler` with the settings each time the Settings window changes them. Call the result to stop. */
  onSettingsChanged(handler: (settings: SettingsSnapshot) => void): Promise<UnlistenFn> {
    return listen(SETTINGS_CHANGED_EVENT, function pass(event: Event<SettingsSnapshot>): void {
      handler(event.payload)
    })
  }

  /**
   * Gives the Settings window the height of its content and shows it. Rust keeps it on the screen.
   * `viewportHeight` is the height the page sees now. Rust uses it to find the title bar.
   */
  fitSettingsWindow(contentHeight: number, viewportHeight: number): Promise<void> {
    return invoke('fit_settings_window', { contentHeight, viewportHeight })
  }

  /** Closes the window of this page. */
  closeWindow(): Promise<void> {
    return getCurrentWindow().close()
  }
}
