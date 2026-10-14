/** How two tables are related by a foreign key. */
export type RelationKind = 'one-to-one' | 'one-to-many' | 'many-to-many'

/** The saved box positions of all diagrams are kept under this key. */
export const DIAGRAM_LAYOUTS_STORAGE_KEY = 'sql-pro.diagram-layouts'

/** The size of a table box, in pixels. The layout and the CSS both use these. */
export const DIAGRAM_NODE_WIDTH = 220
export const DIAGRAM_HEADER_HEIGHT = 26
export const DIAGRAM_ROW_HEIGHT = 20

/** The free room between two boxes in the automatic layout, in pixels. */
export const DIAGRAM_NODE_GAP = 40
/** The free room between two columns of boxes. The lines run here. */
export const DIAGRAM_RANK_GAP = 110

/** Tables with no relation are put in rows of this many boxes. */
export const DIAGRAM_LOOSE_PER_ROW = 5

/** The free room around the diagram in the exported picture, in pixels. */
export const DIAGRAM_EXPORT_PADDING = 24

/** The "Diagram saved" status goes away after this many milliseconds. */
export const DIAGRAM_SAVED_STATUS_MS = 5000

/** The text in the middle of a line. '' = no text. */
export const RELATION_LABELS: Record<RelationKind, string> = {
  'one-to-one': '',
  'one-to-many': '',
  'many-to-many': 'N : M',
}

/** The marks at the two ends of a line: first the table that is pointed at, then the table with the key. */
export const RELATION_MARKS: Record<RelationKind, [string, string]> = {
  'one-to-one': ['1', '1'],
  'one-to-many': ['1', 'N'],
  'many-to-many': ['1', 'N'],
}

/** The words for a kind, for the tooltip of a line. */
export const RELATION_NAMES: Record<RelationKind, string> = {
  'one-to-one': 'One to one',
  'one-to-many': 'One to many',
  'many-to-many': 'Many to many (join table)',
}
