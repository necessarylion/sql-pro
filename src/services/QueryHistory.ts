import { QUERY_HISTORY_MAX_LENGTH, QUERY_HISTORY_STORAGE_KEY } from '../constants/history'
import type { ProfileStorage } from './ProfileStorage'

export interface QueryHistoryEntry {
  id: string
  sql: string
  /** When it ran, as an ISO date. */
  ranAt: string
  /** The title of the tab it ran in. */
  connection: string
}

/**
 * The SQL that was run in the Query tab, with the time of each run.
 * The newest entry is first. The list is kept in the storage of the profile.
 */
export class QueryHistory {
  entries: QueryHistoryEntry[]
  private storage: ProfileStorage

  constructor(storage: ProfileStorage) {
    this.storage = storage
    this.entries = this.read()
  }

  /** Adds the SQL as the newest entry. Empty SQL is ignored. */
  add(sql: string, connection: string): void {
    const clean = sql.trim()
    if (!clean) {
      return
    }
    this.entries.unshift({
      id: crypto.randomUUID(),
      sql: clean,
      ranAt: new Date().toISOString(),
      connection,
    })
    this.entries.length = Math.min(this.entries.length, QUERY_HISTORY_MAX_LENGTH)
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

  clear(): void {
    this.entries = []
    this.write()
  }

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
          typeof item.connection === 'string'
        ) {
          entries.push({
            id: item.id,
            sql: item.sql,
            ranAt: item.ranAt,
            connection: item.connection,
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
