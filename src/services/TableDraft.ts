import { ID_COLUMN_TYPE } from '../constants'
import type { ColumnSpec, NewColumn } from '../types'

/** What the user types for one column. `default` is raw SQL. '' means no default. */
export interface ColumnText {
  name: string
  dataType: string
  nullable: boolean
  default: string
}

/** One column row of the "New table" form. */
export interface ColumnDraft extends ColumnText {
  primaryKey: boolean
  autoIncrement: boolean
}

/**
 * The rules of the column forms: the "add column" form and the "New table" form.
 * It turns what the user typed into what the backend gets.
 */
export class TableDraft {
  /** An empty column row. */
  static emptyColumn(): ColumnDraft {
    return {
      name: '',
      dataType: '',
      nullable: true,
      default: '',
      primaryKey: false,
      autoIncrement: false,
    }
  }

  /** The column a new table starts with: an `id` that the database numbers itself. */
  static idColumn(): ColumnDraft {
    return {
      name: 'id',
      dataType: ID_COLUMN_TYPE,
      nullable: false,
      default: '',
      primaryKey: true,
      autoIncrement: true,
    }
  }

  /** The column as the backend gets it. Spaces around the name and the type are dropped. */
  static spec(text: ColumnText): ColumnSpec {
    return {
      name: text.name.trim(),
      dataType: text.dataType.trim(),
      nullable: text.nullable,
      default: text.default.trim() === '' ? null : text.default,
    }
  }

  /** The number of columns in the primary key. */
  static keyCount(columns: ColumnDraft[]): number {
    let count = 0
    for (const column of columns) {
      if (column.primaryKey) {
        count += 1
      }
    }
    return count
  }

  /**
   * Call this after a "primary key" box changed.
   * A key column cannot hold NULL. Auto increment needs a key of exactly one column.
   */
  static keyChanged(columns: ColumnDraft[]): void {
    const single = TableDraft.keyCount(columns) === 1
    for (const column of columns) {
      if (column.primaryKey) {
        column.nullable = false
      }
      if (!column.primaryKey || !single) {
        column.autoIncrement = false
      }
    }
  }

  /** What is wrong with a new table. '' when it can be sent. */
  static problem(name: string, columns: ColumnDraft[]): string {
    if (name.trim() === '') {
      return 'The table needs a name'
    }
    if (columns.length === 0) {
      return 'The table needs at least one column'
    }
    const single = TableDraft.keyCount(columns) === 1
    const names = new Set<string>()
    for (const column of columns) {
      const spec = TableDraft.spec(column)
      if (spec.name === '') {
        return 'Every column needs a name'
      }
      if (spec.dataType === '') {
        return `Column "${spec.name}" needs a type`
      }
      if (names.has(spec.name)) {
        return `Two columns are named "${spec.name}"`
      }
      names.add(spec.name)
      if (column.autoIncrement && !(column.primaryKey && single)) {
        return 'Auto increment only works on a column that is the whole primary key'
      }
    }
    return ''
  }

  /** The columns of a new table as the backend gets them. */
  static columns(columns: ColumnDraft[]): NewColumn[] {
    const specs: NewColumn[] = []
    for (const column of columns) {
      specs.push({
        ...TableDraft.spec(column),
        primaryKey: column.primaryKey,
        autoIncrement: column.autoIncrement,
      })
    }
    return specs
  }
}
