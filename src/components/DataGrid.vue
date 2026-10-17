<script setup lang="ts">
import {
  computed,
  getCurrentInstance,
  inject,
  nextTick,
  onBeforeUnmount,
  onMounted,
  reactive,
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
  COLUMN_MOVE_THRESHOLD,
  GRID_ROW_BUFFER,
  GRID_ROW_HEIGHT,
  NUMERIC_TYPE_PATTERN,
} from '../constants'
import { ColumnOrder } from '../services/ColumnOrder'
import { RowCopy } from '../services/RowCopy'
import { COLUMN_ORDERS, injectRequired, SHORTCUTS } from '../services/keys'
import { TableSelection } from '../services/TableSelection'
import ContextMenu from './ContextMenu.vue'
import type { GridColumn, Row, RowsPick } from '../types'

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
  /** Give this to save the dragged column order under this table key. See ColumnOrderStore. */
  orderKey?: string
}>()

const emit = defineEmits<{
  sort: [column: string]
  edit: [rowIndex: number, columnIndex: number, value: string | null]
  /** A right-click on a row. The selection is already updated. */
  menu: [event: MouseEvent, rowIndex: number]
}>()

const selected = defineModel<number[]>('selected')
const shortcuts = injectRequired(SHORTCUTS)
// The selection logic. `selected` stays the source: the parent can clear it, like after a reload.
const selection = new TableSelection<number>()
const rowCopy = new RowCopy()
// A parent that listens to `menu` shows its own row menu, with a Copy item that calls `copyRows`.
// Without a listener the grid shows its own menu with Copy only. null = closed.
const ownMenu = !getCurrentInstance()?.vnode.props?.onMenu
const menu = ref<{ x: number; y: number } | null>(null)
defineExpose({ copyRows, pickedRows })
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

// The columns can be dragged to a new place on screen, for example to compare two columns.
// Display only: edits, selection and export use the data column numbers.
const columnOrder = reactive(new ColumnOrder())
const orderStore = inject(COLUMN_ORDERS, null)
/** The data column numbers in shown order. */
const shown = computed(shownColumns)
/** The data column of the header press, and the x where the press started. */
let pressColumn: number | null = null
let pressX = 0
/** The data column that is being moved. A press becomes a move only after a few pixels. */
const moving = ref<number | null>(null)
/** The header the moved column would drop next to, and on which side. */
const dropTarget = ref<{ column: number; after: boolean } | null>(null)
// A floating copy of the moved column follows the mouse. Its box is set once when the move
// starts. Each mouse move only changes its transform, in the next animation frame.
const ghost = useTemplateRef<HTMLElement>('ghost')
const ghostBox = ref<CSSProperties>({})
let ghostShift = 0
let ghostFrame = 0
// Sets, so each row finds out in one step if it is selected or deleted.
const selectedSet = computed(setOfSelected)
const deletedSet = computed(setOfDeleted)
// One regex test per column, not one per cell.
const numericColumns = computed(columnsThatAreNumeric)

// Only the rows that can be seen are drawn. A page has up to 500 rows and a query result
// up to 2000, and drawing all their cells made the grid slow. Empty rows above and below
// keep the scroll height. `start` and `end` are numbers, so a scroll that keeps them the
// same does not draw the grid again.
const scrollTop = ref(0)
const viewHeight = ref(0)
const start = computed(firstDrawnRow)
const end = computed(endOfDrawnRows)
const drawnRows = computed(rowsToDraw)
// The longest text of each column. A hidden row holds it, so the columns get the width
// of all rows, not only of the drawn ones. Without it the widths would jump on scroll.
const widest = computed(widestValues)
let resizeObserver: ResizeObserver | null = null
onMounted(watchSize)
onBeforeUnmount(unwatchSize)

// Another table or another query result starts with automatic widths and the data order,
// or the saved order of that table.
watch(columnsAndKey, resetColumns, { immediate: true })
// The grid can go away in the middle of a drag.
onBeforeUnmount(stopResize)
onBeforeUnmount(cancelMove)

function watchSize(): void {
  if (grid.value) {
    resizeObserver = new ResizeObserver(measure)
    resizeObserver.observe(grid.value)
    measure()
  }
}

function unwatchSize(): void {
  resizeObserver?.disconnect()
}

function measure(): void {
  viewHeight.value = grid.value?.clientHeight ?? 0
}

/** An edit in a row that scrolls out of the drawn part is kept, because its editor goes away. */
function onScroll(): void {
  scrollTop.value = grid.value?.scrollTop ?? 0
  const row = editing.value?.row
  if (row !== undefined && (row < start.value || row >= end.value)) {
    commit(text.value)
  }
}

function firstDrawnRow(): number {
  const first = Math.floor(scrollTop.value / GRID_ROW_HEIGHT) - GRID_ROW_BUFFER
  const stepped = Math.floor(Math.max(0, first) / GRID_ROW_BUFFER) * GRID_ROW_BUFFER
  // A shorter new page can come before the scroll moves back. Never start past the end.
  return Math.min(stepped, props.rows.length)
}

function endOfDrawnRows(): number {
  const last = Math.ceil((scrollTop.value + viewHeight.value) / GRID_ROW_HEIGHT) + GRID_ROW_BUFFER
  const stepped = Math.ceil(last / GRID_ROW_BUFFER) * GRID_ROW_BUFFER
  return Math.max(start.value, Math.min(stepped, props.rows.length))
}

function rowsToDraw(): number[] {
  const indexes: number[] = []
  for (let index = start.value; index < end.value; index += 1) {
    indexes.push(index)
  }
  return indexes
}

/** The cell font is monospace, so the most characters means the widest text. */
function widestValues(): string[] {
  const widest: string[] = []
  for (let column = 0; column < props.columns.length; column += 1) {
    widest.push('')
  }
  for (const row of props.rows) {
    for (const [column, value] of row.entries()) {
      const shown = value === null ? 'NULL' : preview(value)
      if (shown.length > (widest[column]?.length ?? 0)) {
        widest[column] = shown
      }
    }
  }
  return widest
}

function columnsThatAreNumeric(): boolean[] {
  const numeric: boolean[] = []
  for (const column of props.columns) {
    numeric.push(NUMERIC_TYPE_PATTERN.test(column.type ?? ''))
  }
  return numeric
}

function columnNames(): string {
  const names: string[] = []
  for (const column of props.columns) {
    names.push(column.name)
  }
  return names.join('\n')
}

function columnsAndKey(): string {
  return `${props.orderKey ?? ''}\n${columnNames()}`
}

/** The column names in data order. */
function dataNames(): string[] {
  const names: string[] = []
  for (const column of props.columns) {
    names.push(column.name)
  }
  return names
}

function resetColumns(): void {
  widths.value = {}
  columnOrder.reset()
  if (props.orderKey && orderStore) {
    columnOrder.restore(dataNames(), orderStore.load(props.orderKey))
  }
}

/** Saves the shown order of a real table. Other grids have no key and save nothing. */
function saveOrder(): void {
  if (props.orderKey && orderStore) {
    const names = dataNames()
    const shownNames: string[] = []
    for (const index of shown.value) {
      shownNames.push(names[index])
    }
    orderStore.save(props.orderKey, shownNames, names)
  }
}

function shownColumns(): number[] {
  return columnOrder.shown(props.columns.length)
}

/** A press on a header can start a column move. The resize handle has its own drag. */
function startMove(event: PointerEvent, column: number): void {
  if ((event.target as HTMLElement).closest('.resizer')) {
    return
  }
  ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
  pressColumn = column
  pressX = event.clientX
}

/**
 * The capture sends all moves to the pressed header. The drop target is the header
 * under the mouse. The mouse on its left half drops before it, on its right half after it.
 */
function moveColumn(event: PointerEvent): void {
  if (pressColumn === null) {
    return
  }
  if (moving.value === null) {
    if (Math.abs(event.clientX - pressX) < COLUMN_MOVE_THRESHOLD) {
      return
    }
    showGhost(event.currentTarget as HTMLElement, pressColumn)
  }
  ghostShift = event.clientX - pressX
  if (!ghostFrame) {
    ghostFrame = requestAnimationFrame(placeGhost)
  }
  const cells = (event.currentTarget as HTMLElement).parentElement?.children ?? []
  for (const cell of cells) {
    const column = (cell as HTMLElement).dataset.column
    const box = cell.getBoundingClientRect()
    if (column !== undefined && event.clientX >= box.left && event.clientX < box.right) {
      setDropTarget(Number(column), event.clientX > box.left + box.width / 2)
    }
  }
}

/** The ghost starts over the header of the moved column, as tall as the visible grid. */
function showGhost(header: HTMLElement, column: number): void {
  const head = header.getBoundingClientRect()
  const box = grid.value?.getBoundingClientRect()
  ghostShift = 0
  ghostBox.value = {
    left: `${head.left}px`,
    top: `${box?.top ?? head.top}px`,
    width: `${head.width}px`,
    height: `${grid.value?.clientHeight ?? 0}px`,
  }
  moving.value = column
  document.documentElement.classList.add('moving-column')
}

function placeGhost(): void {
  ghostFrame = 0
  if (ghost.value) {
    ghost.value.style.transform = `translateX(${ghostShift}px)`
  }
}

/** Changes the target only when it really changes, so a mouse move draws nothing new. */
function setDropTarget(target: number, after: boolean): void {
  if (target === moving.value) {
    dropTarget.value = null
  } else if (dropTarget.value?.column !== target || dropTarget.value.after !== after) {
    dropTarget.value = { column: target, after }
  }
}

/** Ends the press. A real move puts the column in its new place, and the click after it must not sort. */
function endMove(): void {
  if (moving.value !== null && dropTarget.value) {
    const { column, after } = dropTarget.value
    columnOrder.move(props.columns.length, moving.value, column, after)
    saveOrder()
  }
  if (moving.value !== null) {
    skipSort = true
  }
  cancelMove()
}

function cancelMove(): void {
  pressColumn = null
  moving.value = null
  dropTarget.value = null
  cancelAnimationFrame(ghostFrame)
  ghostFrame = 0
  document.documentElement.classList.remove('moving-column')
}

function dropSide(column: number): string {
  if (dropTarget.value?.column !== column) {
    return ''
  }
  return dropTarget.value.after ? 'drop-after' : 'drop-before'
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
  return deletedSet.value.has(rowIndex)
}

function setOfSelected(): Set<number> {
  return new Set(selected.value)
}

function setOfDeleted(): Set<number> {
  return new Set(props.deleted)
}

/**
 * All values one row is drawn from. `v-memo` draws the row again only when one of them changes.
 * So a selection change draws only the rows that change, not the whole table.
 * The cell values go in one by one: an edit of a new row changes its array in place.
 */
function rowMemo(row: Row, rowIndex: number): unknown[] {
  const edited = editing.value?.row === rowIndex
  return [
    ...row,
    isSelected(rowIndex),
    isNew(rowIndex),
    isDeleted(rowIndex),
    props.changed?.[rowIndex],
    edited ? editing.value?.column : -1,
    edited ? text.value : '',
    props.columns,
    columnStyles.value,
    shown.value,
    moving.value,
  ]
}

function preview(value: string): string {
  return value.length > CELL_PREVIEW_LENGTH ? value.slice(0, CELL_PREVIEW_LENGTH) : value
}

function isNumeric(columnIndex: number): boolean {
  return numericColumns.value[columnIndex] ?? false
}

function sort(column: string): void {
  if (props.orderBy !== undefined && !skipSort) {
    emit('sort', column)
  }
}

function isSelected(rowIndex: number): boolean {
  return selectedSet.value.has(rowIndex)
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
  if (target.closest('tbody tr:not(.spacer), thead, .editor')) {
    return
  }
  if (event.offsetX >= target.clientWidth || event.offsetY >= target.clientHeight) {
    return
  }
  event.preventDefault()
  // The prevented press does not focus the grid. Focus it, so the arrows and Cmd+A work after it.
  grid.value?.focus({ preventScroll: true })
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
 * Cmd+A (Ctrl+A on Windows and Linux) selects all rows, instead of all text of the page.
 * Cmd+C (Ctrl+C) copies the selected rows.
 * Only when the grid itself has the focus. The cell editor keeps its arrows, its Cmd+A and its Cmd+C.
 */
function moveSelection(event: KeyboardEvent): void {
  if (event.target !== event.currentTarget || props.rows.length === 0) {
    return
  }
  if (shortcuts.selectsAll(event)) {
    event.preventDefault()
    selection.names = rowOrder()
    selected.value = [...selection.names]
    return
  }
  // Text the user selected with the mouse wins: then the normal copy runs.
  if (shortcuts.copies(event) && selected.value?.length && !window.getSelection()?.toString()) {
    event.preventDefault()
    copyRows()
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

/**
 * Copies the selected rows as tab-separated text, in grid order and shown column order.
 * Cmd+C is handled as a key, not in a copy event: WebKit sends no copy event when no text is selected.
 */
function copyRows(): void {
  const indexes = [...(selected.value ?? [])].sort(byNumber)
  if (indexes.length === 0) {
    return
  }
  const rows: Row[] = []
  for (const index of indexes) {
    if (props.rows[index]) {
      rows.push(props.rows[index])
    }
  }
  void navigator.clipboard.writeText(rowCopy.text(rows, shown.value))
}

/**
 * The selected rows for "Copy Rows As": the values the grid shows (with unsaved edits),
 * in grid order, with the columns in the shown order.
 */
function pickedRows(): RowsPick {
  const columns: string[] = []
  for (const column of shown.value) {
    columns.push(props.columns[column]?.name ?? '')
  }
  const rows: Row[] = []
  for (const index of [...(selected.value ?? [])].sort(byNumber)) {
    const row = props.rows[index]
    if (!row) {
      continue
    }
    const values: Row = []
    for (const column of shown.value) {
      values.push(row[column] ?? null)
    }
    rows.push(values)
  }
  return { columns, rows }
}

function byNumber(first: number, second: number): number {
  return first - second
}

/**
 * Scrolls the grid so the last selected row can be seen. The row may not be drawn yet,
 * so the place comes from the row height. The sticky header must not hide the row.
 */
function showSelected(): void {
  const index = selected.value?.at(-1)
  const box = grid.value
  if (index === undefined || !box) {
    return
  }
  const header = box.querySelector('thead')?.offsetHeight ?? 0
  const top = index * GRID_ROW_HEIGHT
  if (top < box.scrollTop) {
    box.scrollTop = top
  } else if (header + top + GRID_ROW_HEIGHT > box.scrollTop + box.clientHeight) {
    box.scrollTop = header + top + GRID_ROW_HEIGHT - box.clientHeight
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
  if (ownMenu) {
    menu.value = { x: event.clientX, y: event.clientY }
  } else {
    emit('menu', event, rowIndex)
  }
}

function closeMenu(): void {
  menu.value = null
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
  <div
    ref="grid"
    class="grid"
    tabindex="0"
    @mousedown.left="pressEmpty"
    @keydown="moveSelection"
    @scroll.passive="onScroll"
  >
    <table>
      <thead>
        <tr @pointerdown="allowSort">
          <th
            v-for="columnIndex in shown"
            :key="columnIndex"
            :data-column="columnIndex"
            :class="[
              {
                sortable: orderBy !== undefined,
                numeric: isNumeric(columnIndex),
                moving: moving === columnIndex,
              },
              dropSide(columnIndex),
            ]"
            :style="columnStyles[columnIndex]"
            :title="columns[columnIndex].type"
            @pointerdown.left="startMove($event, columnIndex)"
            @pointermove="moveColumn"
            @pointerup="endMove"
            @pointercancel="cancelMove"
            @click="sort(columns[columnIndex].name)"
          >
            <span class="head">
              <KeyRoundIcon
                v-if="keyColumns?.includes(columns[columnIndex].name)"
                class="key"
                :size="10"
              />
              {{ columns[columnIndex].name }}
              <template v-if="columns[columnIndex].name === orderBy">
                <ChevronDownIcon v-if="descending" :size="10" />
                <ChevronUpIcon v-else :size="10" />
              </template>
            </span>
            <span
              class="resizer"
              :class="{ dragging: resizing === columns[columnIndex].name }"
              title="Drag to resize. Double click to reset."
              @pointerdown.left="startResize($event, columns[columnIndex].name)"
              @pointermove="resize"
              @pointerup="stopResize"
              @pointercancel="stopResize"
              @click.stop
              @dblclick.stop="resetWidth(columns[columnIndex].name)"
            />
          </th>
          <th class="filler" />
        </tr>
      </thead>
      <tbody>
        <tr class="sizer" aria-hidden="true">
          <td v-for="columnIndex in shown" :key="columnIndex">{{ widest[columnIndex] }}</td>
          <td class="filler" />
        </tr>
        <tr v-if="start > 0" class="spacer">
          <td :colspan="columns.length + 1" :style="{ height: `${start * GRID_ROW_HEIGHT}px` }" />
        </tr>
        <tr
          v-for="rowIndex in drawnRows"
          :key="rowIndex"
          v-memo="rowMemo(rows[rowIndex], rowIndex)"
          :class="{
            alt: rowIndex % 2 === 1,
            selected: isSelected(rowIndex),
            draft: isNew(rowIndex),
            deleted: isDeleted(rowIndex),
          }"
          @mousedown.left="press(rowIndex, $event)"
          @mouseenter="dragOver(rowIndex)"
          @contextmenu="openMenu(rowIndex, $event)"
        >
          <td
            v-for="columnIndex in shown"
            :key="columnIndex"
            :class="{
              editing: isEditing(rowIndex, columnIndex),
              numeric: isNumeric(columnIndex),
              modified: isChanged(rowIndex, columnIndex),
              moving: moving === columnIndex,
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
            <span v-else-if="rows[rowIndex][columnIndex] === null" class="null">NULL</span>
            <template v-else>{{ preview(rows[rowIndex][columnIndex] ?? '') }}</template>
          </td>
          <td class="filler" />
        </tr>
        <tr v-if="end < rows.length" class="spacer">
          <td
            :colspan="columns.length + 1"
            :style="{ height: `${(rows.length - end) * GRID_ROW_HEIGHT}px` }"
          />
        </tr>
      </tbody>
    </table>
    <!-- The floating copy of the moved column: its header and its drawn cells. -->
    <div v-if="moving !== null" ref="ghost" class="ghost" :style="ghostBox" aria-hidden="true">
      <div class="ghost-head">{{ columns[moving].name }}</div>
      <div class="ghost-body">
        <div
          class="ghost-rows"
          :class="{ numeric: isNumeric(moving) }"
          :style="{ transform: `translateY(${start * GRID_ROW_HEIGHT - scrollTop}px)` }"
        >
          <div v-for="rowIndex in drawnRows" :key="rowIndex" class="ghost-cell">
            <span v-if="rows[rowIndex][moving] === null" class="null">NULL</span>
            <template v-else>{{ preview(rows[rowIndex][moving] ?? '') }}</template>
          </div>
        </div>
      </div>
    </div>
    <!-- mousedown.stop: a press in the menu must not reach the grid, or it would clear the selection. -->
    <ContextMenu v-if="menu" :x="menu.x" :y="menu.y" @close="closeMenu" @mousedown.stop>
      <button role="menuitem" @click="copyRows">
        Copy
        <span class="hint">{{ shortcuts.hint('C') }}</span>
      </button>
    </ContextMenu>
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

/* The table is at least as wide as the grid, so the header and the rows reach the right edge. */
table {
  min-width: 100%;
  border-spacing: 0;
}

/* Body rows are GRID_ROW_HEIGHT. The row windowing counts on it. The header sets its own height (th below). */
th,
td {
  height: 23px;
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
  height: 28px;
  text-align: center;
  background: var(--header);
  border-bottom: 1px solid var(--border);
  color: var(--text-2);
  font-weight: 600;
}

/* A short column line that stops 7px from the top and bottom, like macOS table headers. */
th::after {
  position: absolute;
  top: 7px;
  right: 0;
  bottom: 7px;
  width: 1px;
  background: var(--border);
  content: '';
  pointer-events: none;
}

th.sortable {
  cursor: pointer;
}

/* The column that is being moved stays dim in its old place. A thin line shows where it will drop. */
th.moving,
td.moving {
  opacity: 0.35;
}

th.drop-before {
  box-shadow: inset 2px 0 var(--accent);
}

th.drop-after {
  box-shadow: inset -2px 0 var(--accent);
}

/* The drag handle on the right edge of a header cell. */
.resizer {
  position: absolute;
  top: 0;
  right: 0;
  bottom: 0;
  z-index: 1;
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

td.numeric {
  text-align: right;
}

/*
  An empty last column that takes the space the columns leave free. A 100% column
  keeps the other columns at their content width. When the columns are wider than
  the grid, it gets no width, so the scroll stays the same.
*/
.filler {
  width: 100%;
  max-width: none;
  padding: 0;
}

th.filler::after {
  content: none;
}

td.filler {
  border-right: 0;
}

/* The last row gets a bottom line, so a short table does not look open. */
tbody tr:last-child td {
  border-bottom: 1px solid var(--border-soft);
}

tbody tr.alt {
  background: var(--row-alt);
}

/*
  The selection colour fades in and out. Only colour, so it stays cheap.
  Only the drawn rows exist (about one screen), and a new row starts with no transition.
*/
tbody tr {
  /* A click selects the row, so the rows show the hand. */
  cursor: pointer;
  transition:
    background-color var(--dur-fast) var(--ease),
    color var(--dur-fast) var(--ease);
}

/* The empty rows that stand for the rows that are not drawn. */
tbody tr.spacer td {
  /* Not a real row, so no hand. */
  cursor: default;
  max-width: none;
  padding: 0;
  border: 0;
}

/* The hidden row with the longest text of each column. It sets the widths but has no height. */
tbody tr.sizer td {
  height: 0;
  border-top: 0;
  border-bottom: 0;
  line-height: 0;
  visibility: hidden;
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
  /* The rows show the hand. The editor keeps the text cursor. */
  cursor: text;
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

/*
  The floating copy of the moved column. Fixed, so it stays over the visible grid
  while the grid scrolls under it. It never takes the mouse.
*/
.ghost {
  position: fixed;
  z-index: 10;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  border: 1px solid var(--accent);
  background: var(--bg);
  box-shadow: var(--popover-shadow);
  opacity: 0.85;
  pointer-events: none;
  will-change: transform;
}

.ghost-head {
  flex: none;
  height: 28px;
  padding: 0 8px;
  overflow: hidden;
  background: var(--header);
  border-bottom: 1px solid var(--border);
  color: var(--text);
  font-weight: 600;
  line-height: 28px;
  text-align: center;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.ghost-body {
  flex: 1;
  min-height: 0;
  overflow: hidden;
}

.ghost-cell {
  height: 23px;
  padding: 0 8px;
  overflow: hidden;
  font-family: var(--font-mono);
  line-height: 23px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.ghost-cell:nth-child(even) {
  background: var(--row-alt);
}

.ghost-rows.numeric .ghost-cell {
  text-align: right;
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
