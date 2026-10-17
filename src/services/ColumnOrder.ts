/**
 * The order a grid shows its columns in. It only changes the display.
 * The data, the edits and the database keep their own column order.
 * Each entry is a column number in the data. The first entry is the first column on screen.
 */
export class ColumnOrder {
  indexes: number[] = []

  /** The data column numbers in shown order. When the count does not match, it is the data order. */
  shown(count: number): number[] {
    if (this.indexes.length === count) {
      return this.indexes
    }
    const order: number[] = []
    for (let index = 0; index < count; index += 1) {
      order.push(index)
    }
    return order
  }

  /**
   * Starts from saved column names. The saved names come first, in their order.
   * Names that no longer exist are skipped. New columns follow in the data order.
   */
  restore(names: string[], saved: string[]): void {
    const order: number[] = []
    for (const name of saved) {
      const index = names.indexOf(name)
      if (index >= 0 && !order.includes(index)) {
        order.push(index)
      }
    }
    for (let index = 0; index < names.length; index += 1) {
      if (!order.includes(index)) {
        order.push(index)
      }
    }
    this.indexes = order
  }

  /** Goes back to the data order. */
  reset(): void {
    this.indexes = []
  }

  /** Puts data column `column` just before (or after) data column `target` on screen. */
  move(count: number, column: number, target: number, after: boolean): void {
    if (column === target) {
      return
    }
    const order = [...this.shown(count)]
    order.splice(order.indexOf(column), 1)
    order.splice(order.indexOf(target) + (after ? 1 : 0), 0, column)
    this.indexes = order
  }
}
