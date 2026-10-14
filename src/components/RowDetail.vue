<script setup lang="ts">
import { computed } from 'vue'
import { XIcon } from 'lucide-vue-next'
import { BINARY_TYPE_PATTERN } from '../constants'
import { FieldValue } from '../services/FieldValue'
import { LAYOUT_SERVICE, SHORTCUTS, injectRequired } from '../services/keys'
import type { GridColumn, Row } from '../types'
import PaneResizer from './PaneResizer.vue'

// The panel on the right of the Content view. It shows one row as a form, one line per column.
// An edit goes to the parent, like an edit in the grid. The parent keeps it as a pending change.
const props = defineProps<{
  columns: GridColumn[]
  /** The shown row, with its pending edits. null = no row is selected. */
  row: Row | null
  /** The number of the row in the grid. The edit event names it. */
  rowIndex: number
  /** False for a view, a table without a primary key, and a row that is marked as deleted. */
  editable: boolean
  /** The column numbers with an unsaved edit. */
  changed: number[]
  /** Like "New row" or "Marked for delete". '' = a plain row. */
  tag: string
}>()

const emit = defineEmits<{
  edit: [rowIndex: number, columnIndex: number, value: string | null]
}>()

const layout = injectRequired(LAYOUT_SERVICE)
const shortcuts = injectRequired(SHORTCUTS)

/** One line of the form. */
interface Field {
  /** The row and column numbers of this render. An edit uses these, not the current selection. */
  row: number
  column: number
  name: string
  type: string
  value: string | null
  editable: boolean
  nullable: boolean
  multiline: boolean
  /** The pretty printed JSON, when it differs from the value. null = no Format button. */
  json: string | null
  changed: boolean
}

const fields = computed(toFields)

function toFields(): Field[] {
  const fields: Field[] = []
  if (!props.row) {
    return fields
  }
  for (const [index, column] of props.columns.entries()) {
    const value = props.row[index] ?? null
    const type = column.type ?? ''
    // The same rules as the cell editor of the grid.
    const editable = props.editable && !column.readOnly && !BINARY_TYPE_PATTERN.test(type)
    const json = FieldValue.prettyJson(value)
    fields.push({
      row: props.rowIndex,
      column: index,
      name: column.name,
      type,
      value,
      editable,
      nullable: column.nullable ?? false,
      multiline: FieldValue.multiline(value),
      json: json === value ? null : json,
      changed: props.changed.includes(index),
    })
  }
  return fields
}

/**
 * Keeps the text of a field when it is left or Enter is pressed. Tells the parent only on a real change.
 * ponytail: like the grid, an untouched NULL stays NULL, so NULL -> '' needs two edits.
 */
function commit(field: Field, event: Event): void {
  const value = (event.target as HTMLInputElement | HTMLTextAreaElement).value
  if (field.value === null && value === '') {
    return
  }
  if (value !== field.value) {
    emit('edit', field.row, field.column, value)
  }
}

function setNull(field: Field): void {
  emit('edit', field.row, field.column, null)
}

function format(field: Field): void {
  if (field.json !== null) {
    emit('edit', field.row, field.column, field.json)
  }
}
</script>

<template>
  <aside class="detail">
    <PaneResizer size="detailWidth" />
    <div class="head">
      <span class="title">{{ row ? `Row ${rowIndex + 1}` : 'Row details' }}</span>
      <span v-if="tag" class="dim">{{ tag }}</span>
      <span class="spacer" />
      <button
        class="icon"
        :title="`Hide row details (${shortcuts.hint('I')})`"
        @click="layout.toggleDetail()"
      >
        <XIcon :size="12" />
      </button>
    </div>
    <div v-if="!row" class="empty dim">Select a row to see its values here.</div>
    <div v-else class="fields">
      <div v-for="field in fields" :key="field.column" class="field">
        <div class="label">
          <span class="name" :title="field.name">{{ field.name }}</span>
          <span class="type" :title="field.type">{{ field.type }}</span>
          <span class="spacer" />
          <button
            v-if="field.editable && field.json !== null"
            class="small"
            title="Pretty print the JSON"
            @click="format(field)"
          >
            Format
          </button>
          <button
            v-if="field.editable && field.nullable && field.value !== null"
            class="small"
            title="Set to NULL"
            @click="setNull(field)"
          >
            NULL
          </button>
        </div>
        <textarea
          v-if="field.multiline"
          :class="{ modified: field.changed }"
          :value="field.value ?? ''"
          :readonly="!field.editable"
          spellcheck="false"
          @change="commit(field, $event)"
        />
        <input
          v-else
          :class="{ modified: field.changed }"
          :value="field.value ?? ''"
          :placeholder="field.value === null ? 'NULL' : ''"
          :readonly="!field.editable"
          spellcheck="false"
          @change="commit(field, $event)"
        />
      </div>
    </div>
  </aside>
</template>

<style scoped>
.detail {
  position: relative;
  display: flex;
  flex: none;
  flex-direction: column;
  width: var(--detail-width);
  /* The grid keeps some room, also with a wide saved width. */
  max-width: calc(100% - 160px);
  min-height: 0;
  background: var(--bg);
  border-left: 1px solid var(--border);
  font-size: 11px;
}

.head {
  display: flex;
  flex: none;
  gap: 6px;
  align-items: center;
  height: 26px;
  padding: 0 4px 0 10px;
  background: var(--header);
  border-bottom: 1px solid var(--border);
  white-space: nowrap;
}

.title {
  color: var(--text-2);
  font-weight: 600;
}

.empty {
  padding: 10px;
}

.fields {
  flex: 1;
  min-height: 0;
  padding: 6px 10px 10px;
  overflow-y: auto;
}

.field + .field {
  margin-top: 6px;
}

.label {
  display: flex;
  gap: 6px;
  align-items: center;
  min-height: 18px;
  white-space: nowrap;
}

.name {
  overflow: hidden;
  color: var(--text);
  font-weight: 600;
  text-overflow: ellipsis;
}

.type {
  flex: 0 1 auto;
  min-width: 0;
  overflow: hidden;
  color: var(--syntax-type);
  font-family: var(--font-mono);
  font-size: 10px;
  text-overflow: ellipsis;
}

.spacer {
  flex: 1;
}

button.small {
  flex: none;
  height: 16px;
  padding: 0 5px;
  border-radius: 3px;
  color: var(--text-2);
  font-size: 9px;
}

input,
textarea {
  display: block;
  width: 100%;
  font-family: var(--font-mono);
  font-size: 11px;
}

input {
  height: 22px;
}

/* A long value grows the box up to a limit. Then it scrolls. */
textarea {
  min-height: 44px;
  max-height: 280px;
  padding: 3px 7px;
  border-radius: 5px;
  resize: vertical;
  field-sizing: content;
}

input[readonly],
textarea[readonly] {
  color: var(--text-2);
}

/* A field with an unsaved edit has the same green as a changed cell in the grid. */
.modified {
  background: var(--added);
}
</style>
