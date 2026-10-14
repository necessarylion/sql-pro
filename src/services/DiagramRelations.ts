import type { RelationKind } from '../constants/diagram'
import type { Diagram, DiagramTable, Relation } from '../types'

/** One line of the diagram. It runs from the table that is pointed at to the table with the key. */
export interface DiagramEdge {
  id: string
  kind: RelationKind
  /** The id of the table the foreign key points at. This is the "one" side. */
  source: string
  /** The first column that is pointed at. The line starts at its row. */
  sourceColumn: string
  /** The id of the table that holds the foreign key. */
  target: string
  /** The first column of the foreign key. The line ends at its row. */
  targetColumn: string
  relation: Relation
}

/** Turns the foreign keys of a database into the lines of the diagram. */
export class DiagramRelations {
  /** The id of a table box. PostgreSQL can have the same name in two schemas. */
  static tableId(schema: string | null, name: string): string {
    return schema ? `${schema}.${name}` : name
  }

  /**
   * One line per foreign key, with its kind.
   * A unique key is one to one. The keys of a join table are many to many. The rest is one to many.
   * A key from or to a table that is not in the diagram is left out.
   */
  static edges(diagram: Diagram): DiagramEdge[] {
    const known = new Set<string>()
    for (const item of diagram.tables) {
      known.add(this.tableId(item.table.schema, item.table.name))
    }
    const joins = this.joinKeys(diagram)
    const edges: DiagramEdge[] = []
    for (const [index, relation] of diagram.relations.entries()) {
      const source = this.tableId(relation.refSchema, relation.refTable)
      const target = this.tableId(relation.schema, relation.table)
      if (!known.has(source) || !known.has(target)) {
        continue
      }
      let kind: RelationKind = 'one-to-many'
      if (relation.unique) {
        kind = 'one-to-one'
      } else if (joins.has(relation)) {
        kind = 'many-to-many'
      }
      edges.push({
        // Two keys can have the same name in MySQL and SQLite. The number keeps the id unique.
        id: `${index}:${target}.${relation.name}`,
        kind,
        source,
        sourceColumn: relation.refColumns[0] ?? '',
        target,
        targetColumn: relation.columns[0] ?? '',
        relation,
      })
    }
    return edges
  }

  /** The foreign key columns of each table: table id -> column names. */
  static foreignColumns(diagram: Diagram): Record<string, string[]> {
    const columns: Record<string, string[]> = {}
    for (const relation of diagram.relations) {
      const id = this.tableId(relation.schema, relation.table)
      columns[id] = [...(columns[id] ?? []), ...relation.columns]
    }
    return columns
  }

  /**
   * The foreign keys of all join tables.
   * A join table has exactly two foreign keys inside its primary key,
   * and the columns of these two are the whole primary key.
   */
  private static joinKeys(diagram: Diagram): Set<Relation> {
    const joins = new Set<Relation>()
    for (const item of diagram.tables) {
      const primary = this.primaryKey(item)
      const inside: Relation[] = []
      const covered = new Set<string>()
      for (const relation of diagram.relations) {
        if (this.holds(item, relation) && this.allIn(relation.columns, primary)) {
          inside.push(relation)
          for (const column of relation.columns) {
            covered.add(column)
          }
        }
      }
      if (inside.length === 2 && primary.size > 0 && covered.size === primary.size) {
        joins.add(inside[0])
        joins.add(inside[1])
      }
    }
    return joins
  }

  private static primaryKey(item: DiagramTable): Set<string> {
    const names = new Set<string>()
    for (const column of item.columns) {
      if (column.primaryKey) {
        names.add(column.name)
      }
    }
    return names
  }

  /** True when `item` is the table that holds the foreign key. */
  private static holds(item: DiagramTable, relation: Relation): boolean {
    return item.table.name === relation.table && item.table.schema === relation.schema
  }

  /** False for an empty list. */
  private static allIn(columns: string[], names: Set<string>): boolean {
    for (const column of columns) {
      if (!names.has(column)) {
        return false
      }
    }
    return columns.length > 0
  }
}
