import { ACCESS_LABELS, ACCESS_ORDER } from '../constants/users'
import type { DbUser, GridColumn, Row } from '../types'

export type UserColumn = 'name' | 'host' | 'access' | 'queryStats'

/** The grid columns in shown order. Host is only there for MySQL. */
const COLUMNS: { key: UserColumn; name: string }[] = [
  { key: 'name', name: 'Name' },
  { key: 'host', name: 'Host' },
  { key: 'access', name: 'Access' },
  { key: 'queryStats', name: 'Query stats' },
]

/** The sort order and the grid text of the Users view. It works on the loaded list. */
export class UserList {
  /** No column keeps the order of the server. */
  column: UserColumn | null = null
  descending = false

  /** A key that stays the same for one account across loads. */
  static key(user: DbUser): string {
    return `${user.name}@${user.host ?? ''}`
  }

  private static shownColumns(hasHosts: boolean): { key: UserColumn; name: string }[] {
    const shown: { key: UserColumn; name: string }[] = []
    for (const column of COLUMNS) {
      if (hasHosts || column.key !== 'host') {
        shown.push(column)
      }
    }
    return shown
  }

  /** The columns of the DataGrid. */
  static columns(hasHosts: boolean): GridColumn[] {
    const columns: GridColumn[] = []
    for (const column of UserList.shownColumns(hasHosts)) {
      columns.push({ name: column.name })
    }
    return columns
  }

  /** One grid row of text. The own account gets a mark after its name. */
  static row(user: DbUser, hasHosts: boolean): Row {
    const row: Row = []
    for (const column of UserList.shownColumns(hasHosts)) {
      if (column.key === 'name') {
        row.push(user.current ? `${user.name} (you)` : user.name)
      } else if (column.key === 'host') {
        row.push(user.host)
      } else if (column.key === 'access') {
        row.push(ACCESS_LABELS[user.access])
      } else {
        row.push(user.queryStats ? 'Yes' : 'No')
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
        this.sortBy(column.key)
      }
    }
  }

  /** A click on the sorted column turns the order around. */
  sortBy(column: UserColumn): void {
    if (this.column === column) {
      this.descending = !this.descending
      return
    }
    this.column = column
    this.descending = false
  }

  /** The users in the sort order. A copy, the list itself is not changed. */
  visible(users: DbUser[]): DbUser[] {
    const shown = [...users]
    return this.column === null ? shown : shown.sort(this.compare.bind(this))
  }

  /**
   * Access sorts from the fewest rights to the most, query stats "No" before "Yes".
   * Text by letters. An empty host goes last in both orders.
   */
  private compare(a: DbUser, b: DbUser): number {
    let order: number
    if (this.column === 'access') {
      order = ACCESS_ORDER.indexOf(a.access) - ACCESS_ORDER.indexOf(b.access)
    } else if (this.column === 'queryStats') {
      order = Number(a.queryStats) - Number(b.queryStats)
    } else {
      const left = a[this.column as 'name' | 'host']
      const right = b[this.column as 'name' | 'host']
      if (left === null || right === null) {
        return Number(left === null) - Number(right === null)
      }
      order = left.localeCompare(right)
    }
    return this.descending ? -order : order
  }
}
