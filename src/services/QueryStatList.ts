import {
  QUERY_STATS_ACCOUNT,
  QUERY_STATS_GUIDES,
  QUERY_STATS_NOT_SET_UP,
  QUERY_STATS_USER,
  type QueryStatsStep,
} from '../constants/queryStats'
import type { Driver, GridColumn, QueryStat, Row, SnippetOutput } from '../types'

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

  /**
   * The rest of the error when it says the stats are not set up: the hint and the server text.
   * null for any other error.
   */
  static setupDetail(error: string): string | null {
    return error.startsWith(QUERY_STATS_NOT_SET_UP)
      ? error.slice(QUERY_STATS_NOT_SET_UP.length).trim()
      : null
  }

  /**
   * The MySQL account from CURRENT_USER(), like app@10.0.0.%, quoted for SQL: 'app'@'10.0.0.%'.
   * A user name may hold an @, so it splits at the last one. null when there is no @.
   */
  static mysqlAccount(currentUser: string): string | null {
    const at = currentUser.lastIndexOf('@')
    if (at < 0) {
      return null
    }
    return `${QueryStatList.quote(currentUser.slice(0, at))}@${QueryStatList.quote(currentUser.slice(at + 1))}`
  }

  /**
   * The setup steps for this database kind, with the user name of the connection in the snippets.
   * `currentUser` is the MySQL CURRENT_USER() text, '' when not known. Then the host stays a word to replace.
   */
  static guide(driver: Driver, user: string, currentUser = ''): QueryStatsStep[] {
    if (driver === 'sqlite') {
      return []
    }
    const name = user || 'user'
    const known = QueryStatList.mysqlAccount(currentUser)
    const account = known ?? `${QueryStatList.quote(name)}@'host'`
    const steps: QueryStatsStep[] = []
    for (const step of QUERY_STATS_GUIDES[driver]) {
      const shown: QueryStatsStep = {
        text: known === null && step.unknownAccountText ? step.unknownAccountText : step.text,
      }
      if (step.code !== undefined) {
        shown.code = QueryStatList.fill(step.code, name, account)
      }
      if (step.run) {
        shown.run = true
      }
      if (step.options) {
        shown.options = []
        for (const option of step.options) {
          shown.options.push({ ...option, code: QueryStatList.fill(option.code, name, account) })
        }
      }
      steps.push(shown)
    }
    return steps
  }

  /**
   * The short text of a snippet output: the error, "Result: 1" for one value, or "Done" with the
   * changed rows. null when the output has more rows or columns, so the guide shows a table.
   */
  static outputText(output: SnippetOutput): string | null {
    if (output.error) {
      return output.error
    }
    const set = output.set
    if (set && set.rows.length > 0) {
      const only = set.rows.length === 1 && set.columns.length === 1
      return only ? `Result: ${set.rows[0]?.[0] ?? 'NULL'}` : null
    }
    const affected = set?.affected ?? 0
    return affected > 0 ? `Done. ${affected} ${affected === 1 ? 'row' : 'rows'} changed.` : 'Done'
  }

  /** Puts the user name and the MySQL account into a snippet. */
  private static fill(code: string, name: string, account: string): string {
    return code.replaceAll(QUERY_STATS_ACCOUNT, account).replaceAll(QUERY_STATS_USER, name)
  }

  /** A SQL string in single quotes. A quote inside is doubled. */
  private static quote(text: string): string {
    return `'${text.replaceAll("'", "''")}'`
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
