import { DIAGRAM_LAYOUTS_STORAGE_KEY } from '../constants/diagram'
import type { ConnectionConfig } from '../types'
import type { ProfileStorage } from './ProfileStorage'

export interface Position {
  x: number
  y: number
}

/** Table id -> the top left corner of its box. */
export type Positions = Record<string, Position>

/**
 * The places of the table boxes that the user moved in a diagram.
 * There is one set per connection and database. It is kept in the storage of the profile.
 */
export class DiagramLayoutStore {
  /** Diagram key -> positions. See `key`. */
  private layouts: Record<string, Positions>
  private storage: ProfileStorage

  constructor(storage: ProfileStorage) {
    this.storage = storage
    this.layouts = this.read()
  }

  /**
   * Names the diagram of a connection, like `mysql:root@127.0.0.1:3306/shop`.
   * For SQLite the database is the file path. A PostgreSQL schema is part of each table id.
   */
  key(config: ConnectionConfig): string {
    return `${config.driver}:${config.user}@${config.host}:${config.port}/${config.database}`
  }

  /** The saved positions of one diagram. Empty when the user moved nothing. */
  positions(key: string): Positions {
    return this.layouts[key] ?? {}
  }

  /** Remembers where the user put one table box. */
  move(key: string, table: string, position: Position): void {
    this.layouts[key] = { ...this.positions(key), [table]: { x: position.x, y: position.y } }
    this.write()
  }

  /** Forgets all saved positions of one diagram. */
  reset(key: string): void {
    delete this.layouts[key]
    this.write()
  }

  private read(): Record<string, Positions> {
    const layouts: Record<string, Positions> = {}
    try {
      // The shape is not trusted. Every part is checked below.
      const saved = JSON.parse(this.storage.getItem(DIAGRAM_LAYOUTS_STORAGE_KEY) ?? '{}') as Record<
        string,
        Record<string, Partial<Position> | null> | null
      > | null
      for (const [key, tables] of Object.entries(saved ?? {})) {
        const positions: Positions = {}
        for (const [table, position] of Object.entries(tables ?? {})) {
          const x = position?.x
          const y = position?.y
          // A position with a wrong shape is left out. The box then gets an automatic place.
          if (typeof x === 'number' && typeof y === 'number') {
            positions[table] = { x, y }
          }
        }
        layouts[key] = positions
      }
    } catch {
      // Broken saved data. Start with no saved positions.
    }
    return layouts
  }

  private write(): void {
    this.storage.setItem(DIAGRAM_LAYOUTS_STORAGE_KEY, JSON.stringify(this.layouts))
  }
}
