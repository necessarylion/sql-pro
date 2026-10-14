<script setup lang="ts">
import { computed, nextTick, onMounted, ref, useTemplateRef, watch } from 'vue'
import { save } from '@tauri-apps/plugin-dialog'
import {
  BaseEdge,
  Handle,
  Position,
  VueFlow,
  getRectOfNodes,
  getSmoothStepPath,
  useVueFlow,
  type Edge,
  type EdgeProps,
  type Node,
  type NodeDragEvent,
  type NodeMouseEvent,
} from '@vue-flow/core'
import '@vue-flow/core/dist/style.css'
import { toBlob } from 'html-to-image'
import { ImageDownIcon, RotateCcwIcon, WorkflowIcon } from 'lucide-vue-next'
import {
  DIAGRAM_EXPORT_PADDING,
  DIAGRAM_HEADER_HEIGHT,
  DIAGRAM_NODE_WIDTH,
  DIAGRAM_ROW_HEIGHT,
  RELATION_LABELS,
  RELATION_MARKS,
  RELATION_NAMES,
} from '../constants/diagram'
import { DiagramLayout } from '../services/DiagramLayout'
import { DiagramRelations, type DiagramEdge } from '../services/DiagramRelations'
import { DIAGRAM_LAYOUTS, injectRequired } from '../services/keys'
import type { Session } from '../services/Session'
import type { ColumnInfo, Diagram } from '../types'
import BottomBar from './BottomBar.vue'

/** What a table box shows. */
interface TableData {
  label: string
  columns: ColumnInfo[]
  /** The names of the foreign key columns. */
  foreign: string[]
}

const props = defineProps<{ session: Session }>()
const layouts = injectRequired(DIAGRAM_LAYOUTS)
// Each tab has its own diagram. The session id keeps them apart.
const flow = useVueFlow(props.session.id)
const canvas = useTemplateRef<HTMLElement>('canvas')
const nodes = ref<Node<TableData>[]>([])
const edges = ref<Edge<DiagramEdge>[]>([])
const count = computed(describeCount)
// The table that was clicked, or null. It and its related tables are shown in green.
const focus = ref<string | null>(null)
const related = computed(relatedTables)

// The sizes of a box, for the CSS. The automatic layout uses the same numbers.
const sizes = {
  '--node-width': `${DIAGRAM_NODE_WIDTH}px`,
  '--node-header': `${DIAGRAM_HEADER_HEIGHT}px`,
  '--node-row': `${DIAGRAM_ROW_HEIGHT}px`,
}

// A query may have changed tables or keys. So load the diagram each time the view opens.
onMounted(reload)
watch(currentDiagram, build, { immediate: true })
flow.onNodesInitialized(fit)

function reload(): void {
  props.session.run(props.session.loadDiagram())
}

function currentDiagram(): Diagram | null {
  return props.session.diagram
}

/** Like "12 tables · 9 relations". */
function describeCount(): string {
  const tables = nodes.value.length
  const relations = edges.value.length
  return `${tables} ${tables === 1 ? 'table' : 'tables'} · ${relations} ${relations === 1 ? 'relation' : 'relations'}`
}

/** The name of this diagram in the saved positions. */
function layoutKey(): string {
  return props.session.config ? layouts.key(props.session.config) : ''
}

/** Shows the schema only when it is not the default one. The same rule as the table list. */
function label(schema: string | null, name: string): string {
  return schema && schema !== 'public' ? `${schema}.${name}` : name
}

/** The clicked table and its related tables, or null when no table is clicked. */
function relatedTables(): Set<string> | null {
  return focus.value ? DiagramRelations.related(edges.value, focus.value) : null
}

/** Clicking a table shows its relations. Clicking it again clears them. */
function toggleFocus(event: NodeMouseEvent): void {
  focus.value = focus.value === event.node.id ? null : event.node.id
}

function clearFocus(): void {
  focus.value = null
}

/** "lit" for a shown table, "dim" for the others, nothing when no table is clicked. */
function tableState(id: string): string {
  if (!related.value) {
    return ''
  }
  return related.value.has(id) ? 'lit' : 'dim'
}

/** "lit" for a line of the clicked table, "dim" for the others. */
function edgeState(edge: EdgeProps): string {
  if (!focus.value) {
    return ''
  }
  return edge.source === focus.value || edge.target === focus.value ? 'lit' : 'dim'
}

/** Makes the boxes and the lines from the loaded diagram. */
function build(): void {
  focus.value = null
  const diagram = props.session.diagram
  const boxes: Node<TableData>[] = []
  const lines: Edge<DiagramEdge>[] = []
  if (diagram) {
    const relations = DiagramRelations.edges(diagram)
    const places = DiagramLayout.positions(diagram, relations, layouts.positions(layoutKey()))
    const foreign = DiagramRelations.foreignColumns(diagram)
    for (const item of diagram.tables) {
      const id = DiagramRelations.tableId(item.table.schema, item.table.name)
      boxes.push({
        id,
        type: 'table',
        position: places[id],
        data: {
          label: label(item.table.schema, item.table.name),
          columns: item.columns,
          foreign: foreign[id] ?? [],
        },
      })
    }
    for (const relation of relations) {
      lines.push({
        id: relation.id,
        type: 'relation',
        class: relation.kind,
        source: relation.source,
        sourceHandle: `out:${relation.sourceColumn}`,
        target: relation.target,
        targetHandle: `in:${relation.targetColumn}`,
        data: relation,
      })
    }
  }
  nodes.value = boxes
  edges.value = lines
}

/** Zooms and moves the view so all boxes can be seen. */
function fit(): void {
  void flow.fitView()
}

/** Remembers the new place of every box that was dragged. */
function savePositions(event: NodeDragEvent): void {
  for (const node of event.nodes) {
    layouts.move(layoutKey(), node.id, node.position)
  }
}

/** Forgets the saved places and puts every box at its automatic place again. */
async function resetLayout(): Promise<void> {
  layouts.reset(layoutKey())
  build()
  await nextTick()
  fit()
}

/** The path of a line and the middle of it. */
function route(edge: EdgeProps): { path: string; x: number; y: number } {
  const [path, x, y] = getSmoothStepPath(edge)
  return { path, x, y }
}

/** The tooltip of a line, like "One to many: users (id) → orders (user_id)". */
function describe(edge: DiagramEdge): string {
  const relation = edge.relation
  const from = `${label(relation.refSchema, relation.refTable)} (${relation.refColumns.join(', ')})`
  const to = `${label(relation.schema, relation.table)} (${relation.columns.join(', ')})`
  return `${RELATION_NAMES[edge.kind]}: ${from} → ${to}`
}

/** Leaves the greyed out tables and lines out of the picture. */
function keepInPicture(element: HTMLElement): boolean {
  return !element.classList?.contains('dim')
}

/** Asks where to save, then writes the picture. Does nothing on cancel. */
async function exportPng(): Promise<void> {
  const session = props.session
  const path = await save({
    title: 'Export diagram',
    defaultPath: session.diagramName(),
    filters: [{ name: 'PNG', extensions: ['png'] }],
  })
  if (path) {
    await session.run(writePng(path))
  }
}

/**
 * Draws all boxes, also the ones outside the window, and saves them as a PNG.
 * ponytail: one picture at zoom 1. A very large diagram can go over the canvas limit
 * of the webview. Then scale the picture down here.
 */
async function writePng(path: string): Promise<void> {
  // The element that Vue Flow moves and zooms. It holds all boxes and lines.
  const pane = canvas.value?.querySelector<HTMLElement>('.vue-flow__transformationpane')
  if (!canvas.value || !pane) {
    throw 'The diagram is not ready'
  }
  // With a clicked table, the picture holds only it and its related tables.
  const shown = []
  for (const node of flow.getNodes.value) {
    if (!related.value || related.value.has(node.id)) {
      shown.push(node)
    }
  }
  const bounds = getRectOfNodes(shown)
  const width = bounds.width + 2 * DIAGRAM_EXPORT_PADDING
  const height = bounds.height + 2 * DIAGRAM_EXPORT_PADDING
  const blob = await toBlob(pane, {
    // The colour of the theme that shows now. So the picture is not see-through.
    backgroundColor: getComputedStyle(canvas.value).backgroundColor,
    filter: keepInPicture,
    width,
    height,
    style: {
      width: `${width}px`,
      height: `${height}px`,
      // Move the top left box into the corner. Ignore the zoom of the screen.
      transform: `translate(${DIAGRAM_EXPORT_PADDING - bounds.x}px, ${DIAGRAM_EXPORT_PADDING - bounds.y}px) scale(1)`,
    },
  })
  if (!blob) {
    throw 'The picture could not be made'
  }
  await props.session.saveImage(path, Array.from(new Uint8Array(await blob.arrayBuffer())))
}
</script>

<template>
  <div class="bar info">
    <WorkflowIcon class="glyph" :size="13" />
    <span class="name">Diagram</span>
    <span v-if="session.diagram" class="dim">{{ count }}</span>
    <span class="spacer" />
    <button
      title="Forget the moved boxes and arrange all tables again"
      :disabled="nodes.length === 0"
      @click="resetLayout"
    >
      <RotateCcwIcon :size="12" />
      Reset layout
    </button>
    <button
      title="Save the whole diagram as a PNG picture"
      :disabled="nodes.length === 0 || session.busy"
      @click="exportPng"
    >
      <ImageDownIcon :size="12" />
      Export PNG
    </button>
  </div>

  <!-- The error text itself is in the bottom bar. -->
  <div v-if="nodes.length === 0" class="empty">
    <template v-if="session.busy">Loading diagram…</template>
    <template v-else-if="!session.diagram">The diagram could not be loaded</template>
    <template v-else>No tables</template>
  </div>
  <div v-else ref="canvas" class="canvas" :style="sizes">
    <VueFlow
      :id="session.id"
      :nodes="nodes"
      :edges="edges"
      :nodes-connectable="false"
      :delete-key-code="null"
      :min-zoom="0.05"
      :max-zoom="2"
      @node-drag-stop="savePositions"
      @node-click="toggleFocus"
      @pane-click="clearFocus"
    >
      <template #node-table="{ id, data }: { id: string; data: TableData }">
        <div class="table" :class="tableState(id)">
          <div class="title">{{ data.label }}</div>
          <div v-for="column in data.columns" :key="column.name" class="column">
            <!-- A line ends at the row of its key column and starts at the row it points at. -->
            <Handle
              :id="`in:${column.name}`"
              type="target"
              :position="Position.Left"
              :connectable="false"
            />
            <span class="column-name" :class="{ key: column.primaryKey }">{{ column.name }}</span>
            <span v-if="column.primaryKey" class="tag pk" title="Primary key">PK</span>
            <span v-if="data.foreign.includes(column.name)" class="tag" title="Foreign key">
              FK
            </span>
            <span class="type">{{ column.dataType }}</span>
            <Handle
              :id="`out:${column.name}`"
              type="source"
              :position="Position.Right"
              :connectable="false"
            />
          </div>
        </div>
      </template>

      <template #edge-relation="edge: EdgeProps<DiagramEdge>">
        <g :class="edgeState(edge)">
          <BaseEdge
            :path="route(edge).path"
            :label="RELATION_LABELS[edge.data.kind]"
            :label-x="route(edge).x"
            :label-y="route(edge).y"
            label-show-bg
            :label-bg-padding="[4, 2]"
            :label-bg-border-radius="3"
          />
          <!-- The marks at the two ends: the "one" side and the side with the key. -->
          <text class="mark" :x="edge.sourceX + 7" :y="edge.sourceY - 5">
            {{ RELATION_MARKS[edge.data.kind][0] }}
          </text>
          <text class="mark end" :x="edge.targetX - 7" :y="edge.targetY - 5">
            {{ RELATION_MARKS[edge.data.kind][1] }}
          </text>
        </g>
        <title>{{ describe(edge.data) }}</title>
      </template>
    </VueFlow>
  </div>

  <BottomBar :session="session" />
</template>

<style scoped>
.info {
  gap: 10px;
  padding: 0 10px;
}

.glyph {
  color: var(--accent-text);
}

.name {
  font-size: 12px;
  font-weight: 600;
}

.info button {
  gap: 5px;
}

.canvas {
  flex: 1;
  min-height: 0;
  background: var(--bg);
}

/* A table box. Its size must match the numbers in constants/diagram.ts. */
.table {
  width: var(--node-width);
  overflow: hidden;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 6px;
  box-shadow: 0 1px 3px #0000001f;
  font-size: 11px;
  cursor: grab;
}

.canvas :deep(.vue-flow__node.selected) .table {
  border-color: var(--accent);
}

.title {
  height: var(--node-header);
  padding: 0 8px;
  overflow: hidden;
  background: var(--header);
  border-bottom: 1px solid var(--border);
  color: var(--text);
  font-weight: 600;
  line-height: calc(var(--node-header) - 1px);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.column {
  position: relative;
  display: flex;
  gap: 5px;
  align-items: center;
  height: var(--node-row);
  padding: 0 8px;
  white-space: nowrap;
}

.column:nth-child(odd) {
  background: var(--row-alt);
}

/* A long name is cut first. The type stays at the right edge. */
.column-name {
  min-width: 0;
  overflow: hidden;
  font-family: var(--font-mono);
  text-overflow: ellipsis;
}

.column-name.key {
  font-weight: 600;
}

/* The small "PK" and "FK" marks. */
.tag {
  flex: none;
  padding: 0 3px;
  border: 1px solid var(--border);
  border-radius: 3px;
  color: var(--text-2);
  font-size: 8px;
  font-weight: 600;
  line-height: 11px;
}

.tag.pk {
  border-color: var(--warn);
  color: var(--warn);
}

.type {
  flex: none;
  max-width: 50%;
  margin-left: auto;
  overflow: hidden;
  color: var(--text-3);
  font-family: var(--font-mono);
  font-size: 10px;
  text-overflow: ellipsis;
}

/* The points where lines attach. They are not shown. Lines cannot be drawn by hand. */
.column :deep(.vue-flow__handle) {
  width: 1px;
  height: 1px;
  min-width: 0;
  min-height: 0;
  border: 0;
  background: none;
  pointer-events: none;
}

.canvas :deep(.vue-flow__edge-path) {
  stroke: var(--text-3);
  stroke-width: 1.2;
}

/* The two keys of a join table. */
.canvas :deep(.many-to-many .vue-flow__edge-path) {
  stroke: var(--accent-text);
  stroke-dasharray: 5 4;
}

.canvas :deep(.vue-flow__edge.selected .vue-flow__edge-path),
.canvas :deep(.vue-flow__edge:hover .vue-flow__edge-path) {
  stroke: var(--accent);
  stroke-width: 2;
}

.canvas :deep(.vue-flow__edge-textbg) {
  fill: var(--bg);
}

.canvas :deep(.vue-flow__edge-text),
.mark {
  fill: var(--text-2);
  font-family: var(--font-mono);
  font-size: 10px;
  font-weight: 600;
}

.canvas :deep(.many-to-many .vue-flow__edge-text) {
  fill: var(--accent-text);
}

.mark.end {
  text-anchor: end;
}

/* A clicked table, its related tables and the lines between them. */
.table.lit {
  border-color: var(--success);
}

.table.lit .title {
  color: var(--success-text);
}

/* ".canvas" makes these win over the many to many, hover and selected colours above. */
.canvas .lit :deep(.vue-flow__edge-path) {
  stroke: var(--success);
  stroke-width: 2;
}

.canvas .lit :deep(.vue-flow__edge-text),
.lit .mark {
  fill: var(--success-text);
}

/* All other tables and lines look disabled. */
.table.dim,
g.dim {
  opacity: 0.3;
}
</style>
