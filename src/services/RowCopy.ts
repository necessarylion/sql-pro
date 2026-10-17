import type { Row } from '../types'

/** A value with a tab, a line break or a quote must be quoted, or it would break the rows and cells. */
const NEEDS_QUOTES = /[\t\n\r"]/

/**
 * Turns grid rows into tab-separated text for the clipboard, the way spreadsheets paste it.
 * One line per row, cells joined by a tab. NULL is written as `NULL`, like the grid shows it.
 */
export class RowCopy {
  /** `rows` in the order to copy. `columns` are the data column numbers in shown order. */
  text(rows: Row[], columns: number[]): string {
    const lines: string[] = []
    for (const row of rows) {
      const cells: string[] = []
      for (const column of columns) {
        cells.push(this.cell(row[column] ?? null))
      }
      lines.push(cells.join('\t'))
    }
    return lines.join('\n')
  }

  /** A value with a tab, line break or quote goes in quotes, and each quote inside is doubled. */
  cell(value: string | null): string {
    if (value === null) {
      return 'NULL'
    }
    if (NEEDS_QUOTES.test(value)) {
      return `"${value.replaceAll('"', '""')}"`
    }
    return value
  }
}
