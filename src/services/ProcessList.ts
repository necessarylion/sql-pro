import type { DbProcess, GridColumn, Row } from '../types'

export type ProcessColumn = keyof DbProcess

/** The grid columns in shown order. A number column gets a numeric type, so it is right aligned. */
const COLUMNS: { key: ProcessColumn; name: string; number: boolean }[] = [
  { key: 'id', name: 'Id', number: true },
  { key: 'user', name: 'User', number: false },
  { key: 'host', name: 'Host', number: false },
  { key: 'database', name: 'Database', number: false },
  { key: 'state', name: 'State', number: false },
  { key: 'seconds', name: 'Time', number: true },
  { key: 'query', name: 'Query', number: false },
  { key: 'current', name: 'Current', number: false },
]
const THIS_TAB = 'this tab'

/** The sort order of the Processes view. It works on the loaded list. */
export class ProcessList {
  /** No column keeps the order of the server. */
  column: ProcessColumn | null = null
  descending = false

  /** Like "12s", "3m 5s" or "2h 4m". */
  static duration(seconds: number | null): string {
    if (seconds === null) {
      return ''
    }
    if (seconds < 60) {
      return `${seconds}s`
    }
    if (seconds < 3600) {
      return `${Math.floor(seconds / 60)}m ${seconds % 60}s`
    }
    return `${Math.floor(seconds / 3600)}h ${Math.floor((seconds % 3600) / 60)}m`
  }

  /** The columns of the DataGrid. */
  static columns(): GridColumn[] {
    const columns: GridColumn[] = []
    for (const column of COLUMNS) {
      columns.push({ name: column.name, type: column.number ? 'numeric' : undefined })
    }
    return columns
  }

  /** The value of one column. Current is "this tab" for the connection of this tab, else empty. */
  static value(process: DbProcess, column: ProcessColumn): string | number | null {
    if (column === 'current') {
      return process.current ? THIS_TAB : null
    }
    return process[column]
  }

  /** One grid row of text. An empty Current cell is '', so it does not show NULL. */
  static row(process: DbProcess): Row {
    const row: Row = []
    for (const column of COLUMNS) {
      const value = ProcessList.value(process, column.key)
      if (value === null) {
        row.push(column.key === 'current' ? '' : null)
      } else if (column.key === 'seconds') {
        row.push(ProcessList.duration(value as number))
      } else {
        row.push(String(value))
      }
    }
    return row
  }

  /** The grid name of the sorted column. '' when nothing is sorted. */
  get columnName(): string {
    for (const column of COLUMNS) {
      if (column.key === this.column) {
        return column.name
      }
    }
    return ''
  }

  /** A click on a grid header. The grid sends the column name. */
  sortByName(name: string): void {
    for (const column of COLUMNS) {
      if (column.name === name) {
        this.sortBy(column.key, column.number)
      }
    }
  }

  /** A click on the sorted column turns the order around. A new number column starts with the biggest first. */
  sortBy(column: ProcessColumn, number = false): void {
    if (this.column === column) {
      this.descending = !this.descending
      return
    }
    this.column = column
    this.descending = number
  }

  /** The processes in the sort order. A copy, the list itself is not changed. */
  visible(processes: DbProcess[]): DbProcess[] {
    const shown = [...processes]
    return this.column === null ? shown : shown.sort(this.compare.bind(this))
  }

  /** Numbers sort by value, text by letters. Empty values go last in both orders. */
  private compare(a: DbProcess, b: DbProcess): number {
    const left = ProcessList.value(a, this.column as ProcessColumn)
    const right = ProcessList.value(b, this.column as ProcessColumn)
    if (left === null || right === null) {
      return Number(left === null) - Number(right === null)
    }
    const order =
      typeof left === 'number' && typeof right === 'number'
        ? left - right
        : String(left).localeCompare(String(right))
    return this.descending ? -order : order
  }
}
