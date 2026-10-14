import type { TableRef } from '../types'

/**
 * The tables that are selected in the sidebar, like the rows of a Finder list.
 * It keeps only the shown names. It does not read the page.
 */
export class TableSelection {
  /** The selected names. */
  names: string[] = []
  /** The name a Shift-click range starts from. null = none yet. */
  anchor: string | null = null

  /** Says in plain words what a list of tables and views is, like "table", "views" or "tables and views". */
  static noun(tables: TableRef[]): string {
    let views = 0
    for (const table of tables) {
      if (table.kind === 'view') {
        views += 1
      }
    }
    const many = tables.length !== 1
    if (views === 0) {
      return many ? 'tables' : 'table'
    }
    if (views === tables.length) {
      return many ? 'views' : 'view'
    }
    return 'tables and views'
  }

  has(name: string): boolean {
    return this.names.includes(name)
  }

  clear(): void {
    this.names = []
    this.anchor = null
  }

  /** A plain click. Nothing stays selected, but the next Shift-click starts here. */
  start(name: string): void {
    this.names = []
    this.anchor = name
  }

  /** Selects only this name. */
  only(name: string): void {
    this.names = [name]
    this.anchor = name
  }

  /** Cmd-click (Ctrl on Windows and Linux): adds the name, or removes it when it is selected. */
  toggle(name: string): void {
    const index = this.names.indexOf(name)
    if (index >= 0) {
      this.names.splice(index, 1)
    } else {
      this.names.push(name)
    }
    this.anchor = name
  }

  /**
   * Shift-click: selects all names from the anchor to this name, in the shown `order`.
   * The anchor stays, so the next Shift-click changes the range. With no anchor, only this name is selected.
   */
  range(order: string[], name: string): void {
    const from = this.anchor === null ? -1 : order.indexOf(this.anchor)
    const to = order.indexOf(name)
    if (from < 0 || to < 0) {
      this.only(name)
      return
    }
    this.names = order.slice(Math.min(from, to), Math.max(from, to) + 1)
  }
}
