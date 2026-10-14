<script setup lang="ts">
import {
  computed,
  nextTick,
  onBeforeUnmount,
  ref,
  useTemplateRef,
  watch,
  type CSSProperties,
} from 'vue'
import { ChevronDownIcon, ChevronUpIcon, KeyRoundIcon } from 'lucide-vue-next'
import {
  BINARY_TYPE_PATTERN,
  CELL_PREVIEW_LENGTH,
  COLUMN_MIN_WIDTH,
  NUMERIC_TYPE_PATTERN,
} from '../constants'
import { injectRequired, SHORTCUTS } from '../services/keys'
import { TableSelection } from '../services/TableSelection'
import type { GridColumn, Row } from '../types'

const props = defineProps<{
  columns: GridColumn[]
  rows: Row[]
  editable?: boolean
  /** Give this (even as '') to make the headers sortable. */
  orderBy?: string
  descending?: boolean
  /** The names of the primary key columns. Their headers get a key icon. */
  keyColumns?: string[]
  /** Rows from this number on are new rows. They are highlighted. */
  newFrom?: number
  /** The cells with an unsaved edit: row number -> column numbers. */
  changed?: Record<number, number[]>
  /** The row numbers that are marked as deleted. They are crossed out and cannot be edited. */
  deleted?: number[]
}>()

const emit = defineEmits<{
  sort: [column: string]
  edit: [rowIndex: number, columnIndex: number, value: string | null]
  /** A right-click on a row. The selection is already updated. */
  menu: [event: MouseEvent]
}>()

const selected = defineModel<number[]>('selected')
const shortcuts = injectRequired(SHORTCUTS)
// The selection logic. `selected` stays the source: the parent can clear it, like after a reload.
const selection = new TableSelection<number>()
const grid = useTemplateRef<HTMLElement>('grid')
// The mouse button can be let go in the middle of a drag when the grid goes away.
onBeforeUnmount(endDrag)

// The cell that is being edited, and the text in its editor.
const editing = ref<{ row: number; column: number } | null>(null)
const text = ref('')

/** Puts the cursor in the editor when it appears. */
const vFocus = {
  mounted(element: HTMLTextAreaElement): void {
    // An editor that would pass the right edge of the grid moves left until it fits,
    // but not past the left edge. This runs before focus, because focus scrolls the
    // grid to show the whole editor.
    const editor = element.parentElement
    const grid = element.closest('.grid')
    if (editor && grid) {
      const box = editor.getBoundingClientRect()
      const left = grid.getBoundingClientRect().left
      const shift = Math.min(box.right - left - grid.clientWidth, box.left - left)
      if (shift > 0) {
        editor.style.left = `${-shift}px`
      }
    }
    element.focus()
    element.select()
  },
}

// The widths the user dragged, by column name. A column without one has its automatic width.
const widths = ref<Record<string, number>>({})
/** The name of the column that is being dragged. */
const resizing = ref('')
// A drag can end with a click on the header. That click must not sort.
let skipSort = false
const columnStyles = computed(stylesOfColumns)

// Another table or another query result starts with automatic widths.
watch(columnNames, resetWidths)
// The grid can go away in the middle of a drag.
onBeforeUnmount(stopResize)

function columnNames(): string {
  const names: string[] = []
  for (const column of props.columns) {
    names.push(column.name)
  }
  return names.join('\n')
}

function resetWidths(): void {
  widths.value = {}
}

/** One style per column. All three are needed: a table cell ignores `width` alone. */
function stylesOfColumns(): (CSSProperties | undefined)[] {
  const styles: (CSSProperties | undefined)[] = []
  for (const column of props.columns) {
    const width = widths.value[column.name]
    const size = `${width}px`
    styles.push(width === undefined ? undefined : { width: size, minWidth: size, maxWidth: size })
  }
  return styles
}

/** The same drag as in PaneResizer: the capture sends all moves to the handle. */
function startResize(event: PointerEvent, column: string): void {
  ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
  resizing.value = column
  document.documentElement.classList.add('resizing')
}

/** The new width is the way from the left edge of the header cell to the mouse. */
function resize(event: PointerEvent): void {
  const header = (event.currentTarget as HTMLElement).parentElement
  if (resizing.value && header) {
    const width = event.clientX - header.getBoundingClientRect().left
    widths.value[resizing.value] = Math.max(COLUMN_MIN_WIDTH, Math.round(width))
  }
}

function stopResize(): void {
  if (resizing.value) {
    resizing.value = ''
    skipSort = true
    document.documentElement.classList.remove('resizing')
  }
}

function allowSort(): void {
  skipSort = false
}

function resetWidth(column: string): void {
  delete widths.value[column]
}

function isChanged(rowIndex: number, columnIndex: number): boolean {
  return props.changed?.[rowIndex]?.includes(columnIndex) ?? false
}

function isNew(rowIndex: number): boolean {
  return props.newFrom !== undefined && rowIndex >= props.newFrom
}

function isDeleted(rowIndex: number): boolean {
  return props.deleted?.includes(rowIndex) ?? false
}

function preview(value: string): string {
  return value.length > CELL_PREVIEW_LENGTH ? value.slice(0, CELL_PREVIEW_LENGTH) : value
}

function isNumeric(columnIndex: number): boolean {
  return NUMERIC_TYPE_PATTERN.test(props.columns[columnIndex]?.type ?? '')
}

function sort(column: string): void {
  if (props.orderBy !== undefined && !skipSort) {
    emit('sort', column)
  }
}

function isSelected(rowIndex: number): boolean {
  return selected.value?.includes(rowIndex) ?? false
}

/** The row numbers in shown order. */
function rowOrder(): number[] {
  const order: number[] = []
  for (let index = 0; index < props.rows.length; index += 1) {
    order.push(index)
  }
  return order
}

/**
 * A press selects one row and starts a drag: moving over rows selects the range.
 * Shift-press selects a range. Cmd-press (Ctrl on Windows and Linux) adds or removes a row.
 * A press in the cell editor is left alone, so its text can be selected.
 */
function press(rowIndex: number, event: MouseEvent): void {
  if ((event.target as HTMLElement).closest('.editor')) {
    return
  }
  selection.names = [...(selected.value ?? [])]
  if (event.shiftKey) {
    selection.range(rowOrder(), rowIndex)
  } else if (shortcuts.hasModifier(event)) {
    selection.toggle(rowIndex)
  } else {
    selection.startDrag(rowIndex)
  }
  selected.value = [...selection.names]
  // Set now, before the browser starts a text selection. The release can be outside the grid.
  document.documentElement.classList.add('selecting')
  window.addEventListener('mouseup', endDrag)
}

/**
 * A press on the empty area below the rows must not start a text selection.
 * Rows, the header and the cell editor handle their own presses.
 * A press on the scrollbar is left alone, so the scrollbar still drags.
 * The press clears the selection (Shift keeps it) and starts a drag: moving over rows selects them.
 */
function pressEmpty(event: MouseEvent): void {
  const target = event.target as HTMLElement
  if (target.closest('tbody tr, thead, .editor')) {
    return
  }
  if (event.offsetX >= target.clientWidth || event.offsetY >= target.clientHeight) {
    return
  }
  event.preventDefault()
  selection.names = [...(selected.value ?? [])]
  selection.startEmptyDrag(event.shiftKey)
  selected.value = [...selection.names]
  document.documentElement.classList.add('selecting')
  window.addEventListener('mouseup', endDrag)
}

function dragOver(rowIndex: number): void {
  if (selection.dragging) {
    selection.dragTo(rowOrder(), rowIndex)
    selected.value = [...selection.names]
  }
}

/**
 * Up and down arrows select the row above or below the last selected row.
 * Only when the grid itself has the focus. The cell editor keeps its arrows.
 */
function moveSelection(event: KeyboardEvent): void {
  if (event.target !== event.currentTarget || props.rows.length === 0) {
    return
  }
  let step = 0
  if (event.key === 'ArrowDown') {
    step = 1
  } else if (event.key === 'ArrowUp') {
    step = -1
  } else {
    return
  }
  event.preventDefault()
  const last = selected.value?.at(-1)
  const next = last === undefined ? 0 : Math.min(props.rows.length - 1, Math.max(0, last + step))
  selection.only(next)
  selected.value = [next]
  void nextTick(showSelected)
}

/** Scrolls the grid so the last selected row can be seen. */
function showSelected(): void {
  const index = selected.value?.at(-1)
  if (index !== undefined) {
    grid.value?.querySelectorAll('tbody tr')[index]?.scrollIntoView({ block: 'nearest' })
  }
}

function endDrag(): void {
  selection.endDrag()
  document.documentElement.classList.remove('selecting')
  window.removeEventListener('mouseup', endDrag)
}

/**
 * A right-click on a row outside the selection selects only that row, like Finder does.
 * The cell editor keeps the native menu, for copy and paste.
 */
function openMenu(rowIndex: number, event: MouseEvent): void {
  if ((event.target as HTMLElement).closest('textarea')) {
    return
  }
  event.preventDefault()
  if (!isSelected(rowIndex)) {
    selected.value = [rowIndex]
  }
  emit('menu', event)
}

function isEditing(rowIndex: number, columnIndex: number): boolean {
  return editing.value?.row === rowIndex && editing.value.column === columnIndex
}

function startEdit(rowIndex: number, columnIndex: number): void {
  const column = props.columns[columnIndex]
  if (
    !props.editable ||
    column.readOnly ||
    isDeleted(rowIndex) ||
    BINARY_TYPE_PATTERN.test(column.type ?? '')
  ) {
    return
  }
  text.value = props.rows[rowIndex][columnIndex] ?? ''
  editing.value = { row: rowIndex, column: columnIndex }
}

function cancel(): void {
  editing.value = null
}

/**
 * Ends the edit. Tells the parent only when the value really changed.
 * Enter removes the editor, and that fires blur. The second call finds
 * `editing` empty and does nothing.
 */
function commit(value: string | null): void {
  const cell = editing.value
  if (!cell) {
    return
  }
  editing.value = null
  const old = props.rows[cell.row]?.[cell.column]
  // ponytail: an untouched NULL cell stays NULL, so NULL -> '' needs two edits.
  const untouchedNull = old === null && value === ''
  if (old !== undefined && value !== old && !untouchedNull) {
    emit('edit', cell.row, cell.column, value)
  }
}

/** Enter saves. Shift+Enter does not reach this and adds a line break. */
function onEnter(event: KeyboardEvent): void {
  // Enter also confirms a word in Chinese or Japanese input. Do not save then.
  if (event.isComposing) {
    return
  }
  event.preventDefault()
  commit(text.value)
}
</script>

<template>
  <div ref="grid" class="grid" tabindex="0" @mousedown.left="pressEmpty" @keydown="moveSelection">
    <table>
      <thead>
        <tr @pointerdown="allowSort">
          <th
            v-for="(column, columnIndex) in columns"
            :key="columnIndex"
            :class="{ sortable: orderBy !== undefined, numeric: isNumeric(columnIndex) }"
            :style="columnStyles[columnIndex]"
            :title="column.type"
            @click="sort(column.name)"
          >
            <span class="head">
              <KeyRoundIcon v-if="keyColumns?.includes(column.name)" class="key" :size="10" />
              {{ column.name }}
              <template v-if="column.name === orderBy">
                <ChevronDownIcon v-if="descending" :size="10" />
                <ChevronUpIcon v-else :size="10" />
              </template>
            </span>
            <span
              class="resizer"
              :class="{ dragging: resizing === column.name }"
              title="Drag to resize. Double click to reset."
              @pointerdown.left="startResize($event, column.name)"
              @pointermove="resize"
              @pointerup="stopResize"
              @pointercancel="stopResize"
              @click.stop
              @dblclick.stop="resetWidth(column.name)"
            />
          </th>
        </tr>
      </thead>
      <tbody>
        <tr
          v-for="(row, rowIndex) in rows"
          :key="rowIndex"
          :class="{
            selected: isSelected(rowIndex),
            draft: isNew(rowIndex),
            deleted: isDeleted(rowIndex),
          }"
          @mousedown.left="press(rowIndex, $event)"
          @mouseenter="dragOver(rowIndex)"
          @contextmenu="openMenu(rowIndex, $event)"
        >
          <td
            v-for="(value, columnIndex) in row"
            :key="columnIndex"
            :class="{
              editing: isEditing(rowIndex, columnIndex),
              numeric: isNumeric(columnIndex),
              modified: isChanged(rowIndex, columnIndex),
            }"
            :style="columnStyles[columnIndex]"
            @dblclick="startEdit(rowIndex, columnIndex)"
          >
            <div v-if="isEditing(rowIndex, columnIndex)" class="editor">
              <textarea
                v-model="text"
                v-focus
                rows="1"
                spellcheck="false"
                @keydown.enter.exact="onEnter"
                @keydown.esc="cancel"
                @blur="commit(text)"
              />
              <!-- mousedown.prevent keeps the focus, so blur does not save first. -->
              <button
                v-if="columns[columnIndex].nullable"
                class="null-button"
                title="Set to NULL"
                @mousedown.prevent="commit(null)"
              >
                NULL
              </button>
            </div>
            <span v-else-if="value === null" class="null">NULL</span>
            <template v-else>{{ preview(value) }}</template>
          </td>
        </tr>
      </tbody>
    </table>
  </div>
</template>

<style scoped>
.grid {
  flex: 1;
  min-height: 0;
  overflow: auto;
  background: var(--bg);
  font-size: 11px;
}

/* The grid takes the focus for the arrow keys. The selected row already shows where it is. */
.grid:focus {
  outline: none;
}

table {
  border-spacing: 0;
}

th,
td {
  height: 22px;
  max-width: 320px;
  padding: 0 8px;
  overflow: hidden;
  text-align: left;
  text-overflow: ellipsis;
  white-space: nowrap;
}

th {
  position: sticky;
  top: 0;
  z-index: 2;
  background: var(--header);
  border-right: 1px solid var(--border);
  border-bottom: 1px solid var(--border);
  color: var(--text-2);
  font-weight: 600;
}

th.sortable {
  cursor: pointer;
}

/* The drag handle on the right edge of a header cell. */
.resizer {
  position: absolute;
  top: 0;
  right: 0;
  bottom: 0;
  width: 6px;
  cursor: col-resize;
}

.resizer:hover::after,
.resizer.dragging::after {
  position: absolute;
  top: 0;
  right: 0;
  bottom: 0;
  width: 2px;
  background: var(--accent);
  content: '';
}

.head {
  display: inline-flex;
  gap: 4px;
  align-items: center;
  vertical-align: middle;
}

.key {
  color: var(--warn);
}

td {
  position: relative;
  border-right: 1px solid var(--border-soft);
  font-family: var(--font-mono);
  -webkit-user-select: text;
  user-select: text;
}

.numeric {
  text-align: right;
}

/* The last row gets a bottom line, so a short table does not look open. */
tbody tr:last-child td {
  border-bottom: 1px solid var(--border-soft);
}

/* A row that the arrow keys scroll to is not hidden under the sticky header. */
tbody tr {
  scroll-margin-top: 22px;
}

tbody tr:nth-child(even) {
  background: var(--row-alt);
}

/* A new row is green. A row that is marked as deleted is red and crossed out. */
tbody tr.draft {
  background: var(--added);
}

tbody tr.deleted {
  background: var(--deleted);
}

tbody tr.deleted td {
  text-decoration: line-through;
}

tbody tr.selected {
  background: var(--accent);
  color: var(--on-accent);
}

/* A selected row keeps a thin line on its left edge that tells what it is. */
tr.selected.draft td:first-child {
  box-shadow: inset 3px 0 var(--success);
}

tr.selected.deleted td:first-child {
  box-shadow: inset 3px 0 var(--danger);
}

/* A cell with an unsaved edit. It stays green in a selected row, with normal text. */
tbody td.modified {
  background: var(--added);
  color: var(--text);
}

tr.selected td.modified .null {
  color: var(--text-3);
  opacity: 1;
}

.null {
  color: var(--text-3);
}

tr.selected .null {
  color: inherit;
  opacity: 0.7;
}

/*
  The editor lies over the cell and the cells next to it. It is absolute, so the
  table keeps its column widths and row heights. A narrow column still gets a
  box that is wide enough to read the text.
*/
td.editing {
  overflow: visible;
}

.editor {
  position: absolute;
  top: 0;
  left: 0;
  z-index: 1;
  display: flex;
  width: max(100%, 220px);
  min-height: 100%;
  border: 1px solid var(--accent);
  outline: 1px solid var(--accent);
  background: var(--bg);
}

textarea {
  flex: 1;
  min-width: 0;
  max-height: 160px;
  padding: 3px 7px;
  border: 0;
  outline: none;
  color: var(--text);
  font-size: 11px;
  text-align: left;
  resize: none;
  field-sizing: content;
}

/* The button stands after the text, so it never covers it. */
.null-button {
  height: 16px;
  margin: 2px 2px 0 0;
  padding: 0 4px;
  border-radius: 3px;
  color: var(--text-2);
  font-family: var(--font-ui);
  font-size: 9px;
}
</style>
