import { COLUMN_ORDER_STORAGE_KEY } from '../constants/columns'
import type { ConnectionConfig, TableRef } from '../types'
import type { ProfileStorage } from './ProfileStorage'

/**
 * The column order the user dragged in the Content view, saved per table.
 * It keeps column names, not numbers, so it still fits after a column is added or dropped.
 * A table in the data order has no entry.
 */
export class ColumnOrderStore {
  /** Table key -> column names in shown order. See `key`. */
  private orders: Record<string, string[]>
  private storage: ProfileStorage

  constructor(storage: ProfileStorage) {
    this.storage = storage
    this.orders = this.read()
  }

  /**
   * Names one table, like `mysql:root@127.0.0.1:3306/shop/public.orders`.
   * The database part is the same as in PinnedTables. For SQLite the database is the file path,
   * so a file opened from the OS gets a key too.
   */
  key(config: ConnectionConfig, table: TableRef): string {
    const database = `${config.driver}:${config.user}@${config.host}:${config.port}/${config.database}`
    return table.schema ? `${database}/${table.schema}.${table.name}` : `${database}/${table.name}`
  }

  /** The saved column names of one table, or an empty list. */
  load(key: string): string[] {
    return this.orders[key] ?? []
  }

  /** Saves the shown order. When it is the data order, the entry is removed instead. */
  save(key: string, shown: string[], data: string[]): void {
    if (shown.join('\n') === data.join('\n')) {
      delete this.orders[key]
    } else {
      this.orders[key] = shown
    }
    this.storage.setItem(COLUMN_ORDER_STORAGE_KEY, JSON.stringify(this.orders))
  }

  private read(): Record<string, string[]> {
    const orders: Record<string, string[]> = {}
    try {
      // The shape is not trusted. Only lists of strings are kept.
      const saved = JSON.parse(this.storage.getItem(COLUMN_ORDER_STORAGE_KEY) ?? '{}') as Record<
        string,
        unknown
      > | null
      for (const [key, names] of Object.entries(saved ?? {})) {
        if (Array.isArray(names) && names.every(isString)) {
          orders[key] = names as string[]
        }
      }
    } catch {
      // Broken saved data. Every table starts in the data order.
    }
    return orders
  }
}

function isString(value: unknown): boolean {
  return typeof value === 'string'
}
