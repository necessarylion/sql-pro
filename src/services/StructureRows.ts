import type { ColumnDetail, ColumnInfo, ForeignKey, Row, TableKeys, TableRef } from '../types'

/**
 * Builds the rows of the Structure grid: one row per table column.
 * The order of the cells is the order of the headers in StructureView.vue:
 * Field, Type, Charset, Collation, Null, Key, Default, Extra, Foreign key, Comment.
 * A value the database does not have is null, so the grid shows NULL.
 */
export class StructureRows {
  rows(columns: ColumnInfo[], keys: TableKeys | null): Row[] {
    const rows: Row[] = []
    for (const column of columns) {
      const detail = this.detail(keys, column.name)
      rows.push([
        column.name,
        column.dataType,
        detail?.charset ?? null,
        detail?.collation ?? null,
        column.nullable ? 'YES' : 'NO',
        column.primaryKey ? 'PRI' : '',
        column.default,
        column.extra,
        this.reference(keys, column.name),
        detail?.comment ?? null,
      ])
    }
    return rows
  }

  /**
   * What the column points at, like `users(id)`. A column of a composite key shows its own
   * part. A column in two foreign keys shows both, joined by a comma. null = no foreign key.
   */
  reference(keys: TableKeys | null, column: string): string | null {
    const parts: string[] = []
    for (const key of keys?.foreignKeys ?? []) {
      const index = key.columns.indexOf(column)
      if (index >= 0) {
        parts.push(`${this.refName(key)}(${key.refColumns[index] ?? ''})`)
      }
    }
    return parts.length > 0 ? parts.join(', ') : null
  }

  /**
   * The table the first foreign key of the column points at, from `tables`.
   * A key without a schema points into the schema of `table`. null = none, or not listed.
   */
  target(
    keys: TableKeys | null,
    column: string,
    table: TableRef,
    tables: TableRef[],
  ): TableRef | null {
    for (const key of keys?.foreignKeys ?? []) {
      if (!key.columns.includes(column)) {
        continue
      }
      const schema = key.refSchema ?? table.schema
      for (const candidate of tables) {
        if (candidate.name === key.refTable && candidate.schema === schema) {
          return candidate
        }
      }
      return null
    }
    return null
  }

  private refName(key: ForeignKey): string {
    return key.refSchema ? `${key.refSchema}.${key.refTable}` : key.refTable
  }

  private detail(keys: TableKeys | null, name: string): ColumnDetail | undefined {
    for (const detail of keys?.columns ?? []) {
      if (detail.name === name) {
        return detail
      }
    }
    return undefined
  }
}
