import type { GridColumn, QueryStat, Row } from '../types'

export type QueryStatColumn = keyof QueryStat

/** The grid columns in shown order. A number column gets a numeric type, so it is right aligned. */
const COLUMNS: { key: QueryStatColumn; name: string; number: boolean }[] = [
  { key: 'query', name: 'Query', number: false },
  { key: 'runs', name: 'Runs', number: true },
  { key: 'lastRun', name: 'Last run', number: false },
  { key: 'totalMs', name: 'Total', number: true },
  { key: 'averageMs', name: 'Average', number: true },
  { key: 'maxMs', name: 'Max', number: true },
  { key: 'rows', name: 'Rows', number: true },
]
const DURATIONS: QueryStatColumn[] = ['totalMs', 'averageMs', 'maxMs']

/** The filter and the sort order of the Query stats view. Both work on the loaded list. */
export class QueryStatList {
  filter = ''
  column: QueryStatColumn = 'totalMs'
  descending = true

  /** Like "0.42 ms", "12 ms" or "3.5 s". */
  static duration(ms: number | null): string {
    if (ms === null) {
      return ''
    }
    if (ms < 1) {
      return `${ms.toFixed(2)} ms`
    }
    if (ms < 1000) {
      return `${Math.round(ms)} ms`
    }
    return `${(ms / 1000).toFixed(1)} s`
  }

  /** The columns of the DataGrid. */
  static columns(): GridColumn[] {
    const columns: GridColumn[] = []
    for (const column of COLUMNS) {
      columns.push({ name: column.name, type: column.number ? 'numeric' : undefined })
    }
    return columns
  }

  /** One grid row of text. Times get a unit. The sort still uses the numbers in the stat. */
  static row(stat: QueryStat): Row {
    const row: Row = []
    for (const column of COLUMNS) {
      const value = stat[column.key]
      if (value === null) {
        row.push(null)
      } else if (DURATIONS.includes(column.key)) {
        row.push(QueryStatList.duration(value as number))
      } else {
        row.push(String(value))
      }
    }
    return row
  }

  /** The grid name of the sorted column. */
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
        this.sortBy(column.key)
      }
    }
  }

  /** A click on the sorted column turns the order around. Another column starts with the biggest first. */
  sortBy(column: QueryStatColumn): void {
    if (this.column === column) {
      this.descending = !this.descending
      return
    }
    this.column = column
    this.descending = column !== 'query'
  }

  /** The stats that match the filter, in the sort order. The filter looks at the query. */
  visible(stats: QueryStat[]): QueryStat[] {
    const needle = this.filter.trim().toLowerCase()
    const shown: QueryStat[] = []
    for (const stat of stats) {
      if (stat.query.toLowerCase().includes(needle)) {
        shown.push(stat)
      }
    }
    return shown.sort(this.compare.bind(this))
  }

  /** Empty values go last in both orders. */
  private compare(a: QueryStat, b: QueryStat): number {
    const left = a[this.column]
    const right = b[this.column]
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
