import { PINNED_TABLES_STORAGE_KEY } from '../constants/pins'
import type { ConnectionConfig } from '../types'
import type { ProfileStorage } from './ProfileStorage'

/**
 * The tables the user pinned in the sidebar.
 * There is one list per connection and database. It is kept in the storage of the profile.
 */
export class PinnedTables {
  /** Database key -> pinned table names. See `key`. */
  private pins: Record<string, string[]>
  private storage: ProfileStorage

  constructor(storage: ProfileStorage) {
    this.storage = storage
    this.pins = this.read()
  }

  /**
   * Names the database of a connection, like `mysql:root@127.0.0.1:3306/shop`.
   * For SQLite the database is the file path. A PostgreSQL schema is part of each table name.
   */
  key(config: ConnectionConfig): string {
    return `${config.driver}:${config.user}@${config.host}:${config.port}/${config.database}`
  }

  /** The pinned table names of one database, in the order they were pinned. */
  list(key: string): string[] {
    return this.pins[key] ?? []
  }

  has(key: string, table: string): boolean {
    return this.list(key).includes(table)
  }

  pin(key: string, table: string): void {
    if (!this.has(key, table)) {
      this.pins[key] = [...this.list(key), table]
      this.write()
    }
  }

  unpin(key: string, table: string): void {
    const left: string[] = []
    for (const name of this.list(key)) {
      if (name !== table) {
        left.push(name)
      }
    }
    if (left.length > 0) {
      this.pins[key] = left
    } else {
      delete this.pins[key]
    }
    this.write()
  }

  private read(): Record<string, string[]> {
    const pins: Record<string, string[]> = {}
    try {
      // The shape is not trusted. Only lists of strings are kept.
      const saved = JSON.parse(this.storage.getItem(PINNED_TABLES_STORAGE_KEY) ?? '{}') as Record<
        string,
        unknown
      > | null
      for (const [key, tables] of Object.entries(saved ?? {})) {
        const names: string[] = []
        for (const name of Array.isArray(tables) ? (tables as unknown[]) : []) {
          if (typeof name === 'string') {
            names.push(name)
          }
        }
        if (names.length > 0) {
          pins[key] = names
        }
      }
    } catch {
      // Broken saved data. Start with no pins.
    }
    return pins
  }

  private write(): void {
    this.storage.setItem(PINNED_TABLES_STORAGE_KEY, JSON.stringify(this.pins))
  }
}
