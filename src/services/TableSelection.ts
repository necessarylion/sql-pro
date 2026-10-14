import type { TableRef } from '../types'

/**
 * The items that are selected in a list, like the rows of a Finder list.
 * The sidebar uses it with table names, the data grid with row numbers.
 * It keeps only the items. It does not read the page.
 */
export class TableSelection<T = string> {
  /** The selected items. */
  names: T[] = []
  /** The item a Shift-click or drag range starts from. null = none yet. */
  anchor: T | null = null
  /** True while the mouse button is held after a plain press. Moving over items then selects a range. */
  dragging = false

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

  has(name: T): boolean {
    return this.names.includes(name)
  }

  clear(): void {
    this.names = []
    this.anchor = null
  }

  /** A plain click. Nothing stays selected, but the next Shift-click starts here. */
  start(name: T): void {
    this.names = []
    this.anchor = name
  }

  /** Selects only this name. */
  only(name: T): void {
    this.names = [name]
    this.anchor = name
  }

  /** Cmd-click (Ctrl on Windows and Linux): adds the name, or removes it when it is selected. */
  toggle(name: T): void {
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
  range(order: T[], name: T): void {
    const from = this.anchor === null ? -1 : order.indexOf(this.anchor)
    const to = order.indexOf(name)
    if (from < 0 || to < 0) {
      this.only(name)
      return
    }
    this.names = order.slice(Math.min(from, to), Math.max(from, to) + 1)
  }

  /** A plain press of the mouse button: selects only this item and starts a drag from it. */
  startDrag(name: T): void {
    this.only(name)
    this.dragging = true
  }

  /**
   * A press on the empty area below the items starts a drag with no start item.
   * It clears the selection, unless `keep` is true (Shift is held): then the drag extends from the old anchor.
   * With no anchor, the first item the mouse moves over becomes the anchor.
   */
  startEmptyDrag(keep: boolean): void {
    if (!keep) {
      this.clear()
    }
    this.dragging = true
  }

  /** The mouse moves over an item while the button is held: selects from the start item to this one. */
  dragTo(order: T[], name: T): void {
    if (this.dragging) {
      this.range(order, name)
    }
  }

  /** The mouse button is let go. The selection stays. */
  endDrag(): void {
    this.dragging = false
  }
}
