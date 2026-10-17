import type { QueryStat } from '../types'

export type QueryStatColumn = keyof QueryStat

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

  /** A click on the sorted column turns the order around. Another column starts with the biggest first. */
  sortBy(column: QueryStatColumn): void {
    if (this.column === column) {
      this.descending = !this.descending
      return
    }
    this.column = column
    this.descending = column !== 'query' && column !== 'database'
  }

  /** The stats that match the filter, in the sort order. The filter looks at the query and the database. */
  visible(stats: QueryStat[]): QueryStat[] {
    const needle = this.filter.trim().toLowerCase()
    const shown: QueryStat[] = []
    for (const stat of stats) {
      const text = `${stat.query}\n${stat.database ?? ''}`.toLowerCase()
      if (text.includes(needle)) {
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
