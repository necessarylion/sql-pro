import { SAVED_QUERIES_STORAGE_KEY } from '../constants/history'
import type { ConnectionConfig } from '../types'
import type { ProfileStorage } from './ProfileStorage'

export interface SavedQuery {
  name: string
  sql: string
  /** When it was last saved, as an ISO date. */
  savedAt: string
}

/**
 * Named SQL the user saved to use again.
 * There is one list per server (see `key`). So a saved connection and an unsaved one
 * to the same server share the list. Names are unique in one list.
 * The lists are kept in the storage of the profile.
 */
export class SavedQueries {
  /** Server key -> saved queries, sorted by name. */
  private lists: Record<string, SavedQuery[]>
  private storage: ProfileStorage

  constructor(storage: ProfileStorage) {
    this.storage = storage
    this.lists = this.read()
  }

  /**
   * Names the server of a connection, like `mysql:root@127.0.0.1:3306`.
   * The database is left out, so the queries stay when the user picks another database.
   * SQLite has no server, so its key is the file path.
   */
  key(config: ConnectionConfig | null): string {
    if (!config) {
      return ''
    }
    if (config.driver === 'sqlite') {
      return `sqlite:${config.database}`
    }
    return `${config.driver}:${config.user}@${config.host}:${config.port}`
  }

  list(key: string): SavedQuery[] {
    return this.lists[key] ?? []
  }

  has(key: string, name: string): boolean {
    return this.find(key, name.trim()) !== null
  }

  /**
   * Saves the SQL under the name. A query with the same name is replaced.
   * Returns false when the name or the SQL is empty. Nothing is saved then.
   */
  save(key: string, name: string, sql: string): boolean {
    const cleanName = name.trim()
    const cleanSql = sql.trim()
    if (!cleanName || !cleanSql) {
      return false
    }
    const queries = this.without(key, cleanName)
    queries.push({ name: cleanName, sql: cleanSql, savedAt: new Date().toISOString() })
    this.store(key, queries)
    return true
  }

  /** Returns false when the new name is empty or another query has it. */
  rename(key: string, name: string, newName: string): boolean {
    const clean = newName.trim()
    const query = this.find(key, name)
    if (!clean || !query) {
      return false
    }
    if (clean === name) {
      return true
    }
    if (this.find(key, clean)) {
      return false
    }
    const queries = this.without(key, name)
    queries.push({ ...query, name: clean })
    this.store(key, queries)
    return true
  }

  remove(key: string, name: string): void {
    this.store(key, this.without(key, name))
  }

  private find(key: string, name: string): SavedQuery | null {
    for (const query of this.list(key)) {
      if (query.name === name) {
        return query
      }
    }
    return null
  }

  /** A copy of the list without the query of that name. */
  private without(key: string, name: string): SavedQuery[] {
    const queries: SavedQuery[] = []
    for (const query of this.list(key)) {
      if (query.name !== name) {
        queries.push(query)
      }
    }
    return queries
  }

  /** Keeps the list sorted by name. An empty list is dropped. */
  private store(key: string, queries: SavedQuery[]): void {
    if (queries.length > 0) {
      queries.sort(SavedQueries.byName)
      this.lists[key] = queries
    } else {
      delete this.lists[key]
    }
    this.write()
  }

  private static byName(a: SavedQuery, b: SavedQuery): number {
    return a.name.localeCompare(b.name)
  }

  private read(): Record<string, SavedQuery[]> {
    const lists: Record<string, SavedQuery[]> = {}
    try {
      // The shape is not trusted. Only entries with the right fields are kept.
      const saved = JSON.parse(this.storage.getItem(SAVED_QUERIES_STORAGE_KEY) ?? '{}') as Record<
        string,
        unknown
      > | null
      for (const [key, items] of Object.entries(saved ?? {})) {
        const queries: SavedQuery[] = []
        for (const item of Array.isArray(items) ? (items as Partial<SavedQuery>[]) : []) {
          if (
            item &&
            typeof item.name === 'string' &&
            typeof item.sql === 'string' &&
            typeof item.savedAt === 'string'
          ) {
            queries.push({ name: item.name, sql: item.sql, savedAt: item.savedAt })
          }
        }
        if (queries.length > 0) {
          lists[key] = queries
        }
      }
    } catch {
      // Broken saved data. Start with no saved queries.
    }
    return lists
  }

  private write(): void {
    this.storage.setItem(SAVED_QUERIES_STORAGE_KEY, JSON.stringify(this.lists))
  }
}
