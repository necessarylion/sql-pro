import type { PlanNode } from '../types'

/** One visible line of the plan tree. */
export interface PlanRow {
  /** The path of the step in the tree, like `0.1.2`. It stays the same while the plan does. */
  key: string
  depth: number
  node: PlanNode
  /** True when the children of the step are shown. */
  open: boolean
}

/** The folding of a query plan tree, and how its numbers are written. The plan never changes. */
export class PlanTree {
  /** The keys of the steps whose children are hidden. */
  private closed = new Set<string>()

  constructor(private nodes: PlanNode[]) {}

  /** The steps to show, from the top down. The children of a closed step are left out. */
  rows(): PlanRow[] {
    const rows: PlanRow[] = []
    this.add(this.nodes, '', 0, rows)
    return rows
  }

  /** Hides the children of the step, or shows them again. */
  toggle(key: string): void {
    if (this.closed.has(key)) {
      this.closed.delete(key)
    } else {
      this.closed.add(key)
    }
  }

  private add(nodes: PlanNode[], prefix: string, depth: number, rows: PlanRow[]): void {
    for (const [index, node] of nodes.entries()) {
      const key = prefix ? `${prefix}.${index}` : String(index)
      const open = !this.closed.has(key)
      rows.push({ key, depth, node, open })
      if (open) {
        this.add(node.children, key, depth + 1, rows)
      }
    }
  }

  /** A row count or cost: whole numbers get thousands separators, others two decimals. */
  static amount(value: number | null): string {
    if (value === null) {
      return ''
    }
    const digits = Number.isInteger(value) || Math.abs(value) >= 100 ? 0 : 2
    return value.toLocaleString('en-US', { maximumFractionDigits: digits })
  }

  /** A time in milliseconds, like `0.25 ms` or `1.3 s`. */
  static time(ms: number | null): string {
    if (ms === null) {
      return ''
    }
    if (ms >= 1000) {
      return `${(ms / 1000).toLocaleString('en-US', { maximumFractionDigits: 2 })} s`
    }
    const digits = ms >= 100 ? 0 : ms >= 1 ? 2 : 3
    return `${ms.toLocaleString('en-US', { maximumFractionDigits: digits })} ms`
  }

  /** The share of a step, like `42%`. Empty when the plan has no measure. */
  static share(share: number | null): string {
    return share === null ? '' : `${Math.round(share * 100)}%`
  }
}
