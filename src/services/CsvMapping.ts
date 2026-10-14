import type { CsvColumn } from '../types'

/** Decides which CSV field fills which table column in the CSV import. */
export class CsvMapping {
  /**
   * The field number for each table column, or null to skip the column.
   * With a header row, a column takes the field with the same name. Case does not matter.
   * Without one, they go by position: the first column takes the first field, and so on.
   */
  guess(columns: string[], firstRow: string[], header: boolean): (number | null)[] {
    const names: string[] = []
    for (const name of firstRow) {
      names.push(name.trim().toLowerCase())
    }
    const fields: (number | null)[] = []
    for (const [index, column] of columns.entries()) {
      if (header) {
        const found = names.indexOf(column.toLowerCase())
        fields.push(found >= 0 ? found : null)
      } else {
        fields.push(index < firstRow.length ? index : null)
      }
    }
    return fields
  }

  /** The names of the fields to choose from: the header names, or "Field 1", "Field 2" and so on. */
  fieldNames(firstRow: string[], header: boolean): string[] {
    const names: string[] = []
    for (const [index, name] of firstRow.entries()) {
      names.push(header && name.trim() ? name : `Field ${index + 1}`)
    }
    return names
  }

  /** How many rows are imported: all rows of the file, without the header row. */
  rowCount(count: number, header: boolean): number {
    return Math.max(0, header ? count - 1 : count)
  }

  /** The columns to send. Skipped columns are left out, so they get their default. */
  chosen(columns: string[], fields: (number | null)[]): CsvColumn[] {
    const chosen: CsvColumn[] = []
    for (const [index, name] of columns.entries()) {
      const field = fields[index]
      if (field !== null && field !== undefined) {
        chosen.push({ name, field })
      }
    }
    return chosen
  }
}
