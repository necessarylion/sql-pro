<script setup lang="ts">
import { computed, reactive, useTemplateRef } from 'vue'
import { confirm } from '@tauri-apps/plugin-dialog'
import { CsvMapping } from '../services/CsvMapping'
import type { Session } from '../services/Session'
import type { CsvImport, TableRef } from '../types'

// The CSV import into one table: which CSV field fills which column, and a short preview.
const props = defineProps<{ session: Session }>()
const dialog = useTemplateRef<HTMLDialogElement>('dialog')
const mapping = new CsvMapping()

const form = reactive({
  table: null as TableRef | null,
  path: '',
  columns: [] as string[],
  /** The field number for each column, or null to skip it. */
  fields: [] as (number | null)[],
  /** The first rows of the file, the header row too. */
  rows: [] as string[][],
  count: 0,
  header: true,
  emptyIsNull: true,
  problem: '',
})

const fieldNames = computed(readFieldNames)
const previewRows = computed(readPreviewRows)
const rowCount = computed(readRowCount)
const ready = computed(canImport)

/** Shows the schema only when it is not the default one. */
function label(table: TableRef): string {
  return table.schema && table.schema !== 'public' ? `${table.schema}.${table.name}` : table.name
}

function fileName(): string {
  return form.path.split(/[\\/]/).pop() ?? form.path
}

/** Opens the dialog for the CSV file at `path`, then loads the columns and the preview. */
async function openFile(table: TableRef, path: string): Promise<void> {
  form.table = table
  form.path = path
  form.columns = []
  form.fields = []
  form.rows = []
  form.count = 0
  form.header = true
  form.emptyIsNull = true
  form.problem = ''
  dialog.value?.showModal()
  try {
    const [columns, preview] = await Promise.all([
      props.session.listColumns(table),
      props.session.previewCsv(path),
    ])
    for (const column of columns) {
      form.columns.push(column.name)
    }
    form.rows = preview.rows
    form.count = preview.count
    guess()
  } catch (error) {
    form.problem = String(error)
  }
}

/** Matches the columns to the fields again: by name with a header row, else by position. */
function guess(): void {
  form.fields = mapping.guess(form.columns, form.rows[0] ?? [], form.header)
}

function readFieldNames(): string[] {
  return mapping.fieldNames(form.rows[0] ?? [], form.header)
}

function readPreviewRows(): string[][] {
  return form.header ? form.rows.slice(1) : form.rows
}

function readRowCount(): number {
  return mapping.rowCount(form.count, form.header)
}

function canImport(): boolean {
  return rowCount.value > 0 && mapping.chosen(form.columns, form.fields).length > 0
}

function close(): void {
  dialog.value?.close()
}

/** Asks to confirm, then imports. The dialog stays open on an error and shows it. */
async function submit(): Promise<void> {
  const table = form.table
  if (!table) {
    return
  }
  const session = props.session
  const rows = rowCount.value
  const sure = await confirm(
    `Import ${rows} ${rows === 1 ? 'row' : 'rows'} into table "${label(table)}"?`,
    { title: 'Import CSV', kind: 'warning' },
  )
  if (!sure) {
    return
  }
  const request: CsvImport = {
    table,
    path: form.path,
    header: form.header,
    emptyIsNull: form.emptyIsNull,
    columns: mapping.chosen(form.columns, form.fields),
  }
  if (await session.run(session.importCsv(request))) {
    close()
  } else {
    form.problem = session.error
  }
}

defineExpose({ openFile })
</script>

<template>
  <dialog ref="dialog" class="import-dialog">
    <form class="panel" @submit.prevent="submit">
      <h2>{{ form.table ? `Import CSV into "${label(form.table)}"` : 'Import CSV' }}</h2>
      <p class="note file" :title="form.path">{{ fileName() }}</p>
      <div class="options">
        <label class="check">
          <input v-model="form.header" type="checkbox" @change="guess" /> First row is header
        </label>
        <label class="check">
          <input v-model="form.emptyIsNull" type="checkbox" /> Empty field is NULL
        </label>
      </div>
      <div class="list">
        <label v-for="(column, index) in form.columns" :key="column" class="map">
          <span class="name" :title="column">{{ column }}</span>
          <select v-model="form.fields[index]">
            <option :value="null">Skip</option>
            <option v-for="(name, field) in fieldNames" :key="field" :value="field">
              {{ name }}
            </option>
          </select>
        </label>
      </div>
      <div v-if="form.rows.length" class="preview">
        <table>
          <thead>
            <tr>
              <th v-for="(name, field) in fieldNames" :key="field">{{ name }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(row, index) in previewRows" :key="index">
              <td v-for="(cell, field) in row" :key="field">{{ cell }}</td>
            </tr>
          </tbody>
        </table>
      </div>
      <p class="note">
        {{ rowCount }} {{ rowCount === 1 ? 'row' : 'rows' }} in one transaction. Skipped columns get
        their default.
      </p>
      <p v-if="form.problem" class="error">{{ form.problem }}</p>
      <div class="actions">
        <button type="button" @click="close">Cancel</button>
        <button type="submit" class="primary" :disabled="!ready || session.busy">Import…</button>
      </div>
    </form>
  </dialog>
</template>

<style scoped>
.import-dialog {
  width: 380px;
  max-width: calc(100vw - 32px);
  padding: 0;
  background: var(--toolbar);
  border: 1px solid var(--border);
  border-radius: 10px;
  box-shadow: 0 12px 32px #00000040;
  color: var(--text);
}

.import-dialog::backdrop {
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
  font-size: 13px;
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.options {
  display: flex;
  gap: 12px;
  align-items: center;
}

.check {
  display: flex;
  gap: 6px;
  align-items: center;
  font-size: 12px;
}

.check input {
  flex: none;
  width: 13px;
  height: 13px;
  margin: 0;
  padding: 0;
  accent-color: var(--accent);
}

/* Many columns scroll inside the dialog. The buttons stay in view. */
.list {
  display: flex;
  flex-direction: column;
  gap: 3px;
  max-height: 30vh;
  padding: 6px 8px;
  overflow-y: auto;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 6px;
}

.map {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 8px;
  align-items: center;
  font-size: 12px;
}

.map select {
  min-width: 0;
  height: 20px;
  font-size: 12px;
}

.name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.preview {
  max-height: 20vh;
  overflow: auto;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 6px;
}

.preview table {
  border-collapse: collapse;
  font-family: var(--font-mono);
  font-size: 11px;
}

.preview th,
.preview td {
  max-width: 140px;
  padding: 2px 6px;
  overflow: hidden;
  text-align: left;
  text-overflow: ellipsis;
  white-space: nowrap;
  border-bottom: 1px solid var(--border);
}

.preview th {
  color: var(--text-2);
  font-weight: 500;
}

.note {
  margin: 0;
  color: var(--text-2);
  font-size: 11px;
}

.file {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
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
