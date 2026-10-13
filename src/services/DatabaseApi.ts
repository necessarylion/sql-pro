import { invoke } from '@tauri-apps/api/core'
import type {
  BrowseRequest,
  ColumnSpec,
  ConnectionConfig,
  ResultSet,
  TablePage,
  TableRef,
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

  browse(id: string, request: BrowseRequest): Promise<TablePage> {
    return invoke('browse', { id, request })
  }

  runQuery(id: string, sql: string): Promise<ResultSet[]> {
    return invoke('run_query', { id, sql })
  }

  updateRow(id: string, table: TableRef, key: Values, changes: Values): Promise<number> {
    return invoke('update_row', { id, table, key, changes })
  }

  insertRow(id: string, table: TableRef, values: Values): Promise<number> {
    return invoke('insert_row', { id, table, values })
  }

  deleteRows(id: string, table: TableRef, keys: Values[]): Promise<number> {
    return invoke('delete_rows', { id, table, keys })
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

  dropTable(id: string, table: TableRef): Promise<void> {
    return invoke('drop_table', { id, table })
  }

  truncateTable(id: string, table: TableRef): Promise<void> {
    return invoke('truncate_table', { id, table })
  }

  /** Writes a SQL dump of the current database to the file at `path`. */
  dumpDatabase(id: string, path: string): Promise<void> {
    return invoke('dump_database', { id, path })
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
