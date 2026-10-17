import { COPY_JSON_INDENT } from '../constants/copy'
import type { CopyFormat, Row, RowsPick } from '../types'

/** A value with a tab, a line break or a quote must be quoted, or it would break the rows and cells. */
const NEEDS_QUOTES = /[\t\n\r"]/
/** A CSV field with a comma, a quote or a line break is quoted (RFC 4180). */
const CSV_NEEDS_QUOTES = /[,"\r\n]/
/** A line break in any form. */
const LINE_BREAK = /\r\n|\r|\n/g

/**
 * Turns grid rows into text for the clipboard.
 * `text` is tab-separated, the way spreadsheets paste it. `as` gives the other formats
 * of "Copy Rows As". INSERT statements need the database, so the Session builds those.
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

  /** The picked rows in one format. Every format but `insert`. */
  as(format: Exclude<CopyFormat, 'insert'>, pick: RowsPick): string {
    switch (format) {
      case 'text':
        return this.text(pick.rows, this.allColumns(pick))
      case 'json':
        return this.json(pick)
      case 'html':
        return this.html(pick)
      case 'markdown':
        return this.markdown(pick)
      case 'csv':
        return this.csv(pick, false)
      case 'csvHeader':
        return this.csv(pick, true)
    }
  }

  /**
   * An array of objects, indented by two spaces. NULL becomes null.
   * Built by hand, so the keys keep the column order, also names like "1".
   */
  json(pick: RowsPick): string {
    if (pick.rows.length === 0) {
      return '[]'
    }
    const pad = ' '.repeat(COPY_JSON_INDENT)
    const objects: string[] = []
    for (const row of pick.rows) {
      const fields: string[] = []
      for (const [index, name] of pick.columns.entries()) {
        fields.push(`${pad}${pad}${JSON.stringify(name)}: ${JSON.stringify(row[index] ?? null)}`)
      }
      objects.push(`${pad}{\n${fields.join(',\n')}\n${pad}}`)
    }
    return `[\n${objects.join(',\n')}\n]`
  }

  /** A table with the names in `<thead>`. Text is escaped. NULL is an empty cell. */
  html(pick: RowsPick): string {
    const head: string[] = []
    for (const name of pick.columns) {
      head.push(`<th>${this.escapeHtml(name)}</th>`)
    }
    const lines = [
      '<table>',
      '  <thead>',
      `    <tr>${head.join('')}</tr>`,
      '  </thead>',
      '  <tbody>',
    ]
    for (const row of pick.rows) {
      const cells: string[] = []
      for (const index of pick.columns.keys()) {
        cells.push(`<td>${this.escapeHtml(row[index] ?? '')}</td>`)
      }
      lines.push(`    <tr>${cells.join('')}</tr>`)
    }
    lines.push('  </tbody>', '</table>')
    return lines.join('\n')
  }

  /** A Markdown table. `|` is escaped and a line break becomes `<br>`. NULL is written as NULL. */
  markdown(pick: RowsPick): string {
    const head: string[] = []
    const rule: string[] = []
    for (const name of pick.columns) {
      head.push(this.markdownText(name))
      rule.push('---')
    }
    const lines = [this.markdownLine(head), this.markdownLine(rule)]
    for (const row of pick.rows) {
      const cells: string[] = []
      for (const index of pick.columns.keys()) {
        cells.push(this.markdownCell(row[index] ?? null))
      }
      lines.push(this.markdownLine(cells))
    }
    return lines.join('\n')
  }

  /** RFC 4180 lines, joined by CRLF. NULL is an empty field. */
  csv(pick: RowsPick, header: boolean): string {
    const lines: string[] = []
    if (header) {
      lines.push(this.csvLine(pick.columns))
    }
    for (const row of pick.rows) {
      lines.push(this.csvLine(row.slice(0, pick.columns.length)))
    }
    return lines.join('\r\n')
  }

  private csvLine(cells: (string | null)[]): string {
    const fields: string[] = []
    for (const cell of cells) {
      const text = cell ?? ''
      fields.push(CSV_NEEDS_QUOTES.test(text) ? `"${text.replaceAll('"', '""')}"` : text)
    }
    return fields.join(',')
  }

  private markdownCell(value: string | null): string {
    if (value === null) {
      return 'NULL'
    }
    return this.markdownText(value)
  }

  private markdownText(value: string): string {
    return value.replaceAll('|', '\\|').replace(LINE_BREAK, '<br>')
  }

  /** Cells that are already escaped. */
  private markdownLine(cells: string[]): string {
    return `| ${cells.join(' | ')} |`
  }

  private escapeHtml(text: string): string {
    return text
      .replaceAll('&', '&amp;')
      .replaceAll('<', '&lt;')
      .replaceAll('>', '&gt;')
      .replaceAll('"', '&quot;')
  }

  private allColumns(pick: RowsPick): number[] {
    return [...pick.columns.keys()]
  }
}
