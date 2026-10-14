import dagre from '@dagrejs/dagre'
import {
  DIAGRAM_HEADER_HEIGHT,
  DIAGRAM_LOOSE_PER_ROW,
  DIAGRAM_NODE_GAP,
  DIAGRAM_NODE_WIDTH,
  DIAGRAM_RANK_GAP,
  DIAGRAM_ROW_HEIGHT,
} from '../constants/diagram'
import type { Diagram, DiagramTable } from '../types'
import type { Positions } from './DiagramLayoutStore'
import { DiagramRelations, type DiagramEdge } from './DiagramRelations'

/** Finds a place for every table box of the diagram. */
export class DiagramLayout {
  /** The height of a table box, in pixels. The 2 is its top and bottom border. */
  static height(item: DiagramTable): number {
    return DIAGRAM_HEADER_HEIGHT + item.columns.length * DIAGRAM_ROW_HEIGHT + 2
  }

  /**
   * The top left corner of every box: table id -> position.
   * A box the user moved keeps its saved place. The others get an automatic place:
   * related tables side by side from left to right, tables with no relation in rows below.
   */
  static positions(diagram: Diagram, edges: DiagramEdge[], saved: Positions): Positions {
    const graph = new dagre.graphlib.Graph()
    graph.setGraph({ rankdir: 'LR', nodesep: DIAGRAM_NODE_GAP, ranksep: DIAGRAM_RANK_GAP })
    const heights: Record<string, number> = {}
    for (const item of diagram.tables) {
      heights[DiagramRelations.tableId(item.table.schema, item.table.name)] = this.height(item)
    }
    for (const edge of edges) {
      // A table that points at itself needs no room of its own.
      if (edge.source !== edge.target) {
        graph.setNode(edge.source, { width: DIAGRAM_NODE_WIDTH, height: heights[edge.source] })
        graph.setNode(edge.target, { width: DIAGRAM_NODE_WIDTH, height: heights[edge.target] })
        graph.setEdge(edge.source, edge.target, {})
      }
    }
    dagre.layout(graph)

    const positions: Positions = {}
    let bottom = 0
    for (const id of graph.nodes()) {
      // dagre gives the middle of a box.
      const node = graph.node(id)
      positions[id] = { x: node.x - DIAGRAM_NODE_WIDTH / 2, y: node.y - heights[id] / 2 }
      bottom = Math.max(bottom, positions[id].y + heights[id] + DIAGRAM_NODE_GAP)
    }

    // The tables with no relation: rows of boxes below the rest.
    let column = 0
    let rowHeight = 0
    for (const id of Object.keys(heights)) {
      if (positions[id]) {
        continue
      }
      if (column === DIAGRAM_LOOSE_PER_ROW) {
        bottom += rowHeight + DIAGRAM_NODE_GAP
        column = 0
        rowHeight = 0
      }
      positions[id] = { x: column * (DIAGRAM_NODE_WIDTH + DIAGRAM_NODE_GAP), y: bottom }
      rowHeight = Math.max(rowHeight, heights[id])
      column += 1
    }

    for (const id of Object.keys(positions)) {
      positions[id] = saved[id] ?? positions[id]
    }
    return positions
  }
}
