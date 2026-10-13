<script setup lang="ts">
import { ref } from 'vue'
import { ChevronDownIcon, ChevronUpIcon, KeyRoundIcon, PlusIcon } from 'lucide-vue-next'
import { BINARY_TYPE_PATTERN, CELL_PREVIEW_LENGTH, NUMERIC_TYPE_PATTERN } from '../constants'
import type { GridColumn, Row } from '../types'

const props = defineProps<{
  columns: GridColumn[]
  rows: Row[]
  editable?: boolean
  /** Give this (even as '') to make the headers sortable. */
  orderBy?: string
  descending?: boolean
  /** Shows the narrow first column that marks the new row. */
  gutter?: boolean
  /** The names of the primary key columns. Their headers get a key icon. */
  keyColumns?: string[]
  /** The row to highlight as new. */
  draftIndex?: number
}>()

const emit = defineEmits<{
  sort: [column: string]
  edit: [rowIndex: number, columnIndex: number, value: string | null]
}>()

const selected = defineModel<number[]>('selected')

// The cell that is being edited, and the text in its editor.
const editing = ref<{ row: number; column: number } | null>(null)
const text = ref('')

/** Puts the cursor in the editor when it appears. */
const vFocus = {
  mounted(element: HTMLTextAreaElement): void {
    element.focus()
    element.select()
  },
}

function preview(value: string): string {
  return value.length > CELL_PREVIEW_LENGTH ? value.slice(0, CELL_PREVIEW_LENGTH) : value
}

function isNumeric(columnIndex: number): boolean {
  return NUMERIC_TYPE_PATTERN.test(props.columns[columnIndex]?.type ?? '')
}

function sort(column: string): void {
  if (props.orderBy !== undefined) {
    emit('sort', column)
  }
}

function isSelected(rowIndex: number): boolean {
  return selected.value?.includes(rowIndex) ?? false
}

/** A click selects one row. Cmd/Ctrl-click adds or removes a row. */
function select(rowIndex: number, event: MouseEvent): void {
  if (!event.metaKey && !event.ctrlKey) {
    selected.value = [rowIndex]
    return
  }
  const next = [...(selected.value ?? [])]
  const position = next.indexOf(rowIndex)
  if (position < 0) {
    next.push(rowIndex)
  } else {
    next.splice(position, 1)
  }
  selected.value = next
}

function isEditing(rowIndex: number, columnIndex: number): boolean {
  return editing.value?.row === rowIndex && editing.value.column === columnIndex
}

function startEdit(rowIndex: number, columnIndex: number): void {
  const column = props.columns[columnIndex]
  if (!props.editable || column.readOnly || BINARY_TYPE_PATTERN.test(column.type ?? '')) {
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
  <div class="grid">
    <table>
      <thead>
        <tr>
          <th v-if="gutter" class="gutter"></th>
          <th
            v-for="(column, columnIndex) in columns"
            :key="columnIndex"
            :class="{ sortable: orderBy !== undefined, numeric: isNumeric(columnIndex) }"
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
          </th>
        </tr>
      </thead>
      <tbody>
        <tr
          v-for="(row, rowIndex) in rows"
          :key="rowIndex"
          :class="{ selected: isSelected(rowIndex), draft: rowIndex === draftIndex }"
          @click="select(rowIndex, $event)"
        >
          <td v-if="gutter" class="gutter">
            <PlusIcon v-if="rowIndex === draftIndex" :size="10" />
          </td>
          <td
            v-for="(value, columnIndex) in row"
            :key="columnIndex"
            :class="{ editing: isEditing(rowIndex, columnIndex), numeric: isNumeric(columnIndex) }"
            @dblclick="startEdit(rowIndex, columnIndex)"
          >
            <template v-if="isEditing(rowIndex, columnIndex)">
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
            </template>
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

tbody tr:nth-child(even) {
  background: var(--row-alt);
}

tbody tr.draft {
  background: var(--added);
}

tbody tr.selected {
  background: var(--accent);
  color: var(--on-accent);
}

.gutter {
  width: 26px;
  min-width: 26px;
  padding: 0;
  color: var(--success);
  text-align: center;
  -webkit-user-select: none;
  user-select: none;
}

.gutter svg {
  vertical-align: middle;
}

tr.selected .gutter {
  color: inherit;
}

.null {
  color: var(--text-3);
}

tr.selected .null {
  color: inherit;
  opacity: 0.7;
}

/* The editor lies over the cell. It may grow past the cell for long text. */
td.editing {
  overflow: visible;
}

textarea {
  position: absolute;
  top: 0;
  left: 0;
  z-index: 1;
  width: 100%;
  min-width: 120px;
  min-height: 100%;
  max-height: 160px;
  padding: 3px 7px;
  border-color: var(--accent);
  outline: 1px solid var(--accent);
  color: var(--text);
  font-size: 11px;
  text-align: left;
  resize: none;
  field-sizing: content;
}

.null-button {
  position: absolute;
  top: 3px;
  right: 3px;
  z-index: 1;
  height: 16px;
  padding: 0 4px;
  border-radius: 3px;
  color: var(--text-2);
  font-family: var(--font-ui);
  font-size: 9px;
}
</style>
