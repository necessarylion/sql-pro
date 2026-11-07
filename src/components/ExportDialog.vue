<script setup lang="ts">
import { computed, reactive, useTemplateRef } from 'vue'
import { open as openFolder, save } from '@tauri-apps/plugin-dialog'
import type { Session } from '../services/Session'
import { SchemaName } from '../services/SchemaName'
import { EXPORT_FORMAT_LABELS } from '../constants/export'
import type { ExportFormat, ExportRequest, TableRef } from '../types'

// The export dialog. Two kinds:
// - many tables, as one SQL file, or as one CSV, JSON or Excel file per table in a folder
// - one table with chosen columns, as one SQL, CSV, JSON or Excel file
const props = defineProps<{ session: Session }>()
const dialog = useTemplateRef<HTMLDialogElement>('dialog')

// `table` is set for the column export. null means the export of many tables.
const form = reactive({
  table: null as TableRef | null,
  /** The labels of the checked tables. */
  tables: [] as string[],
  columns: [] as string[],
  allColumns: [] as string[],
  format: 'sql' as ExportFormat,
  structure: true,
  data: true,
  problem: '',
})

const ready = computed(canExport)
const kind = computed(formatLabel)

/** The name of the chosen format, like "Excel". */
function formatLabel(): string {
  return EXPORT_FORMAT_LABELS[form.format]
}

/** Shows the schema only when it is not the default one. */
function label(table: TableRef): string {
  return SchemaName.label(table.schema, table.name)
}

/** Opens the export of many tables. `checked` are checked at the start. */
function openTables(checked: TableRef[]): void {
  form.table = null
  form.tables = []
  for (const table of checked) {
    form.tables.push(label(table))
  }
  form.format = 'sql'
  form.structure = true
  form.data = true
  form.problem = ''
  dialog.value?.showModal()
}

/** Opens the export of one table. All its columns are checked at the start. */
async function openColumns(table: TableRef): Promise<void> {
  form.table = table
  form.columns = []
  form.allColumns = []
  form.format = 'csv'
  form.problem = ''
  dialog.value?.showModal()
  try {
    for (const column of await props.session.listColumns(table)) {
      form.allColumns.push(column.name)
    }
    form.columns = [...form.allColumns]
  } catch (error) {
    form.problem = String(error)
  }
}

function close(): void {
  dialog.value?.close()
}

function checkAll(): void {
  if (form.table) {
    form.columns = [...form.allColumns]
  } else {
    form.tables = []
    for (const table of props.session.tables) {
      form.tables.push(label(table))
    }
  }
}

function checkNone(): void {
  if (form.table) {
    form.columns = []
  } else {
    form.tables = []
  }
}

function canExport(): boolean {
  if (form.table) {
    return form.columns.length > 0
  }
  const something = form.format !== 'sql' || form.structure || form.data
  return form.tables.length > 0 && something
}

/** The checked columns in the order of the table, not in the order they were clicked. */
function chosenColumns(): string[] {
  const picked: string[] = []
  for (const name of form.allColumns) {
    if (form.columns.includes(name)) {
      picked.push(name)
    }
  }
  return picked
}

/** The checked tables in the order of the list. */
function chosenTables(): TableRef[] {
  const picked: TableRef[] = []
  for (const table of props.session.tables) {
    if (form.tables.includes(label(table))) {
      picked.push(table)
    }
  }
  return picked
}

/** Tables as CSV, JSON or Excel go into a folder. Everything else is one file. null on cancel. */
async function pickPath(tables: TableRef[]): Promise<string | null> {
  if (!form.table && form.format !== 'sql') {
    return openFolder({ title: `Export tables as ${kind.value} into this folder`, directory: true })
  }
  return save({
    title: `Export as ${kind.value}`,
    defaultPath: props.session.exportName(tables, form.format),
    filters: [{ name: kind.value, extensions: [form.format] }],
  })
}

/** Asks where to save, then exports. The dialog stays open on an error and shows it. */
async function submit(): Promise<void> {
  const session = props.session
  const tables = form.table ? [form.table] : chosenTables()
  const path = await pickPath(tables)
  if (!path) {
    return
  }
  const request: ExportRequest = {
    tables,
    format: form.format,
    // The column export writes rows only.
    structure: !form.table && form.structure,
    data: !!form.table || form.data,
    columns: form.table ? chosenColumns() : [],
    path,
  }
  if (await session.run(session.exportTables(request))) {
    close()
  } else {
    form.problem = session.error
  }
}

defineExpose({ openTables, openColumns })
</script>

<template>
  <dialog ref="dialog" class="export-dialog">
    <form class="panel" @submit.prevent="submit">
      <h2>{{ form.table ? `Export "${label(form.table)}"` : 'Export tables' }}</h2>
      <div class="head">
        <span>{{ form.table ? 'Columns' : 'Tables' }}</span>
        <button type="button" class="link" @click="checkAll">All</button>
        <button type="button" class="link" @click="checkNone">None</button>
      </div>
      <div class="list">
        <template v-if="form.table">
          <label v-for="name in form.allColumns" :key="name" class="check">
            <input v-model="form.columns" type="checkbox" :value="name" />
            <span class="name">{{ name }}</span>
          </label>
        </template>
        <template v-else>
          <label v-for="table in session.tables" :key="label(table)" class="check">
            <input v-model="form.tables" type="checkbox" :value="label(table)" />
            <span class="name">{{ label(table) }}</span>
            <span v-if="table.kind === 'view'" class="kind">view</span>
          </label>
        </template>
      </div>
      <div class="options">
        <label v-for="(name, format) in EXPORT_FORMAT_LABELS" :key="format" class="check">
          <input v-model="form.format" type="radio" :value="format" /> {{ name }}
        </label>
      </div>
      <div v-if="form.format === 'sql' && !form.table" class="options">
        <label class="check"><input v-model="form.structure" type="checkbox" /> Structure</label>
        <label class="check"><input v-model="form.data" type="checkbox" /> Data</label>
      </div>
      <p class="note">
        <template v-if="form.table">
          {{
            form.format === 'sql'
              ? 'INSERT statements with the checked columns.'
              : `One ${kind} file.`
          }}
        </template>
        <template v-else-if="form.format !== 'sql'"
          >One {{ kind }} file per table, in a folder.</template
        >
        <template v-else>One SQL file.</template>
      </p>
      <p v-if="form.problem" class="error">{{ form.problem }}</p>
      <div class="actions">
        <button type="button" @click="close">Cancel</button>
        <button type="submit" class="primary" :disabled="!ready || session.busy">Export…</button>
      </div>
    </form>
  </dialog>
</template>

<style scoped>
.export-dialog {
  width: 320px;
  max-width: calc(100vw - 32px);
  padding: 0;
  background: var(--toolbar);
  border: 1px solid var(--border);
  border-radius: 10px;
  box-shadow: 0 12px 32px #00000040;
  color: var(--text);
}

.export-dialog::backdrop {
  background: #00000040;
}

.panel {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 14px 16px 16px;
}

h2 {
  margin: 0;
  overflow: hidden;
  font-size: calc(13px * var(--font-scale));
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.head {
  display: flex;
  gap: 8px;
  align-items: center;
  color: var(--text-2);
}

.head span {
  flex: 1;
}

.link {
  height: 18px;
  padding: 0 6px;
  border: 0;
  background: none;
  color: var(--accent-text);
  font-size: calc(11px * var(--font-scale));
}

/* Many tables scroll inside the dialog. The buttons stay in view. */
.list {
  display: flex;
  flex-direction: column;
  gap: 2px;
  max-height: 40vh;
  padding: 6px 8px;
  overflow-y: auto;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 6px;
}

.check {
  display: flex;
  gap: 6px;
  align-items: center;
  min-width: 0;
  font-size: calc(12px * var(--font-scale));
}

.check input {
  flex: none;
  width: 13px;
  height: 13px;
  margin: 0;
  padding: 0;
  accent-color: var(--accent);
}

.name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.kind {
  color: var(--text-3);
  font-size: calc(11px * var(--font-scale));
}

.options {
  display: flex;
  gap: 12px;
  align-items: center;
}

.note {
  margin: 0;
  color: var(--text-2);
  font-size: calc(11px * var(--font-scale));
}

.error {
  margin: 0;
  overflow-wrap: anywhere;
  white-space: pre-wrap;
}

.actions {
  display: flex;
  gap: 8px;
  justify-content: flex-end;
  padding-top: 4px;
}
</style>
