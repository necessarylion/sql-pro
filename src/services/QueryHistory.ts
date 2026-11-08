import {
  QUERY_HISTORY_MAX_LENGTH,
  QUERY_HISTORY_STORAGE_KEY,
  QUERY_HISTORY_TOTAL_MAX_LENGTH,
} from '../constants/history'
import type { ConnectionConfig } from '../types'
import type { ProfileStorage } from './ProfileStorage'

export interface QueryHistoryEntry {
  id: string
  sql: string
  /** When it ran, as an ISO date. */
  ranAt: string
  /** The title of the tab it ran in. */
  connection: string
  /** The database it ran on (see `QueryHistory.key`). */
  database: string
}

/**
 * The SQL that was run in the Query tab, with the time of each run.
 * Each entry belongs to one database. The newest entry is first.
 * The list is kept in the storage of the profile.
 */
export class QueryHistory {
  /** The entries of all databases, newest first. */
  entries: QueryHistoryEntry[]
  private storage: ProfileStorage

  constructor(storage: ProfileStorage) {
    this.storage = storage
    this.entries = this.read()
  }

  /**
   * Names the database of a connection, like `mysql:127.0.0.1:3306/shop`.
   * The user is left out, so two accounts on one database share the history.
   * SQLite has no server, so its key is the file path.
   */
  key(config: ConnectionConfig | null): string {
    if (!config) {
      return ''
    }
    if (config.driver === 'sqlite') {
      return `sqlite:${config.database}`
    }
    return `${config.driver}:${config.host}:${config.port}/${config.database}`
  }

  /** The entries of one database, newest first. */
  list(database: string): QueryHistoryEntry[] {
    const entries: QueryHistoryEntry[] = []
    for (const entry of this.entries) {
      if (entry.database === database) {
        entries.push(entry)
      }
    }
    return entries
  }

  /** Adds the SQL as the newest entry of the database. Empty SQL is ignored. */
  add(sql: string, database: string, connection: string): void {
    const clean = sql.trim()
    if (!clean) {
      return
    }
    this.entries.unshift({
      id: crypto.randomUUID(),
      sql: clean,
      ranAt: new Date().toISOString(),
      connection,
      database,
    })
    const ofDatabase = this.list(database)
    if (ofDatabase.length > QUERY_HISTORY_MAX_LENGTH) {
      // Only one entry is added at a time, so at most the oldest one is too many.
      this.entries.splice(this.entries.lastIndexOf(ofDatabase[ofDatabase.length - 1]), 1)
    }
    this.entries.length = Math.min(this.entries.length, QUERY_HISTORY_TOTAL_MAX_LENGTH)
    this.write()
  }

  remove(id: string): void {
    for (const [index, entry] of this.entries.entries()) {
      if (entry.id === id) {
        this.entries.splice(index, 1)
        this.write()
        return
      }
    }
  }

  /** Removes the entries of one database. The others stay. */
  clear(database: string): void {
    const kept: QueryHistoryEntry[] = []
    for (const entry of this.entries) {
      if (entry.database !== database) {
        kept.push(entry)
      }
    }
    this.entries = kept
    this.write()
  }

  /** Old entries saved without a database are dropped, because their database is not known. */
  private read(): QueryHistoryEntry[] {
    const entries: QueryHistoryEntry[] = []
    try {
      const saved: unknown = JSON.parse(this.storage.getItem(QUERY_HISTORY_STORAGE_KEY) ?? '[]')
      for (const item of Array.isArray(saved) ? saved : []) {
        if (
          item &&
          typeof item.id === 'string' &&
          typeof item.sql === 'string' &&
          typeof item.ranAt === 'string' &&
          typeof item.connection === 'string' &&
          typeof item.database === 'string'
        ) {
          entries.push({
            id: item.id,
            sql: item.sql,
            ranAt: item.ranAt,
            connection: item.connection,
            database: item.database,
          })
        }
      }
    } catch {
      // Broken saved data. Start with an empty list.
    }
    return entries
  }

  private write(): void {
    this.storage.setItem(QUERY_HISTORY_STORAGE_KEY, JSON.stringify(this.entries))
  }
}
