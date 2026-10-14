<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { confirm } from '@tauri-apps/plugin-dialog'
import { LockIcon, MinusIcon, TableIcon, Trash2Icon } from 'lucide-vue-next'
import { DEFAULT_REFERENTIAL_ACTION, REFERENTIAL_ACTIONS } from '../constants/keys'
import type { Session } from '../services/Session'
import { TableDraft } from '../services/TableDraft'
import type {
  ColumnSpec,
  ForeignKey,
  GridColumn,
  IndexInfo,
  ReferentialAction,
  Row,
  TablePage,
  TableRef,
} from '../types'
import BottomBar from './BottomBar.vue'
import ColumnPicker from './ColumnPicker.vue'
import DataGrid from './DataGrid.vue'
import PaneResizer from './PaneResizer.vue'

// Key and Extra cannot be changed here. Default can be set to "no default" with NULL.
const HEADERS: GridColumn[] = [
  { name: 'Field' },
  { name: 'Type' },
  { name: 'Null' },
  { name: 'Key', readOnly: true },
  { name: 'Default', nullable: true },
  { name: 'Extra', readOnly: true },
]

const props = defineProps<{ session: Session }>()
const rows = computed(structureRows)
const rowCount = computed(describeRowCount)
const selected = ref<number[]>([])

// The form for a new column.
const newName = ref('')
const newType = ref('')
const newNullable = ref(true)
const newDefault = ref('')

// The form for a new index.
const indexName = ref('')
const indexColumns = ref<string[]>([])
const indexUnique = ref(false)

// The form for a new foreign key.
const keyName = ref('')
const keyColumns = ref<string[]>([])
const keyRefTable = ref<TableRef | null>(null)
const keyRefColumns = ref<string[]>([])
/** The columns of the picked referenced table. */
const refTableColumns = ref<string[]>([])
const keyOnDelete = ref<ReferentialAction>(DEFAULT_REFERENTIAL_ACTION)
const keyOnUpdate = ref<ReferentialAction>(DEFAULT_REFERENTIAL_ACTION)

const columnNames = computed(tableColumnNames)
const refTables = computed(plainTables)
const canAddKey = computed(foreignKeyReady)

// Row numbers mean other columns after a reload. Drop the selection.
watch(sessionPage, clearSelection)
watch(keyRefTable, pickRefTable)
onMounted(loadKeys)

/** The keys are loaded with the page, but only while this view shows. */
function loadKeys(): void {
  if (!props.session.keys) {
    props.session.run(props.session.loadKeys())
  }
}

function tableColumnNames(): string[] {
  const names: string[] = []
  for (const column of props.session.page?.columns ?? []) {
    names.push(column.name)
  }
  return names
}

/** A foreign key can only point at a table, not at a view. */
function plainTables(): TableRef[] {
  const tables: TableRef[] = []
  for (const table of props.session.tables) {
    if (table.kind === 'table') {
      tables.push(table)
    }
  }
  return tables
}

function foreignKeyReady(): boolean {
  const count = keyColumns.value.length
  return (
    keyName.value.trim() !== '' &&
    keyRefTable.value !== null &&
    count > 0 &&
    count === keyRefColumns.value.length
  )
}

/** PostgreSQL can have the same table name in two schemas. Show the schema then. */
function tableLabel(table: TableRef): string {
  return table.schema ? `${table.schema}.${table.name}` : table.name
}

function references(key: ForeignKey): string {
  const table = key.refSchema ? `${key.refSchema}.${key.refTable}` : key.refTable
  return `${table} (${key.refColumns.join(', ')})`
}

/** An empty part is an expression. */
function indexColumnsText(index: IndexInfo): string {
  const parts: string[] = []
  for (const column of index.columns) {
    parts.push(column === '' ? '(expression)' : column)
  }
  return parts.join(', ')
}

function pickRefTable(table: TableRef | null): void {
  props.session.run(loadRefColumns(table))
}

async function loadRefColumns(table: TableRef | null): Promise<void> {
  keyRefColumns.value = []
  refTableColumns.value = []
  if (!table) {
    return
  }
  const names: string[] = []
  for (const column of await props.session.listColumns(table)) {
    names.push(column.name)
  }
  // The user may have picked another table while this one was loading.
  if (keyRefTable.value === table) {
    refTableColumns.value = names
  }
}

function addIndex(): void {
  if (indexName.value.trim() && indexColumns.value.length > 0) {
    props.session.run(sendNewIndex())
  }
}

/** The form is cleared only when the index was added. */
async function sendNewIndex(): Promise<void> {
  await props.session.addIndex({
    name: indexName.value,
    columns: indexColumns.value,
    unique: indexUnique.value,
  })
  indexName.value = ''
  indexColumns.value = []
  indexUnique.value = false
}

async function dropIndex(index: IndexInfo): Promise<void> {
  const sure = await confirm(`Drop index "${index.name}"?`, {
    title: 'Drop index',
    kind: 'warning',
  })
  if (sure) {
    await props.session.dropIndex(index.name)
  }
}

function addForeignKey(): void {
  if (canAddKey.value) {
    props.session.run(sendNewForeignKey())
  }
}

/** The form is cleared only when the foreign key was added. */
async function sendNewForeignKey(): Promise<void> {
  const refTable = keyRefTable.value
  if (!refTable) {
    return
  }
  await props.session.addForeignKey({
    name: keyName.value,
    columns: keyColumns.value,
    refTable,
    refColumns: keyRefColumns.value,
    onDelete: keyOnDelete.value,
    onUpdate: keyOnUpdate.value,
  })
  keyName.value = ''
  keyColumns.value = []
  keyRefTable.value = null
  keyOnDelete.value = DEFAULT_REFERENTIAL_ACTION
  keyOnUpdate.value = DEFAULT_REFERENTIAL_ACTION
}

async function dropForeignKey(key: ForeignKey): Promise<void> {
  const sure = await confirm(`Drop foreign key "${key.name}"?`, {
    title: 'Drop foreign key',
    kind: 'warning',
  })
  if (sure) {
    await props.session.dropForeignKey(key.name)
  }
}

function sessionPage(): TablePage | null {
  return props.session.page
}

function clearSelection(): void {
  selected.value = []
}

/** One grid row per column of the table. */
function structureRows(): Row[] {
  const rows: Row[] = []
  for (const column of props.session.page?.columns ?? []) {
    rows.push([
      column.name,
      column.dataType,
      column.nullable ? 'YES' : 'NO',
      column.primaryKey ? 'PRI' : '',
      column.default,
      column.extra,
    ])
  }
  return rows
}

/** With a filter on, the total counts only the rows that match. */
function describeRowCount(): string {
  const total = (props.session.page?.total ?? 0).toLocaleString()
  return props.session.filtered ? `${total} matching rows` : `${total} rows`
}

function edit(rowIndex: number, columnIndex: number, value: string | null): void {
  props.session.run(changeColumn(rowIndex, columnIndex, value))
}

/**
 * Changes one thing of a column. A grid row is a table column here.
 * The rest of the spec is sent back as it is, so the backend leaves it alone.
 */
async function changeColumn(
  rowIndex: number,
  columnIndex: number,
  value: string | null,
): Promise<void> {
  const column = props.session.page?.columns[rowIndex]
  if (!column) {
    return
  }
  const spec: ColumnSpec = {
    name: column.name,
    dataType: column.dataType,
    nullable: column.nullable,
    default: column.default,
  }
  const text = (value ?? '').trim()
  const field = HEADERS[columnIndex].name
  const name = `"${column.name}"`
  // The question says in plain words what the change does.
  let question: string
  if (field === 'Default') {
    spec.default = text === '' ? null : value
    const old = column.default ?? 'no default'
    question = `Change default of ${name} from ${old} to ${spec.default ?? 'no default'}?`
  } else if (field === 'Null') {
    const answer = text.toUpperCase()
    if (answer !== 'YES' && answer !== 'NO') {
      throw 'Type YES or NO in the Null column'
    }
    spec.nullable = answer === 'YES'
    question = spec.nullable
      ? `Allow NULL in ${name}?`
      : `Make ${name} NOT NULL? This can rewrite the whole table. It fails when the column holds NULL.`
  } else if (text === '') {
    throw `The ${field === 'Field' ? 'name' : 'type'} of a column cannot be empty`
  } else if (field === 'Field') {
    spec.name = text
    question = `Rename column ${name} to "${text}"?`
  } else {
    spec.dataType = text
    question = `Change type of ${name} from ${column.dataType} to ${text}? This can rewrite the whole table and may lose data.`
  }
  if (await confirm(question, { title: 'Change column', kind: 'warning' })) {
    await props.session.alterColumn(column.name, spec)
  }
}

function addColumn(): void {
  if (newName.value.trim() && newType.value.trim()) {
    props.session.run(sendNewColumn())
  }
}

/** The form is cleared only when the column was added. */
async function sendNewColumn(): Promise<void> {
  await props.session.addColumn(
    TableDraft.spec({
      name: newName.value,
      dataType: newType.value,
      nullable: newNullable.value,
      default: newDefault.value,
    }),
  )
  newName.value = ''
  newType.value = ''
  newNullable.value = true
  newDefault.value = ''
}

async function dropSelected(): Promise<void> {
  const column = props.session.page?.columns[selected.value[0]]
  if (!column) {
    return
  }
  const sure = await confirm(`Drop column "${column.name}"? All data in it is lost.`, {
    title: 'Drop column',
    kind: 'warning',
  })
  if (sure) {
    await props.session.dropColumn(column.name)
  }
}
</script>

<template>
  <div class="bar info">
    <TableIcon class="glyph" :size="13" />
    <span class="name">{{ session.table?.name }}</span>
    <span class="dim">{{ rows.length }} columns</span>
    <span class="dim">{{ rowCount }}</span>
    <span class="spacer" />
    <span v-if="!session.editable" class="pill">
      <LockIcon :size="10" />
      Rows are read only: {{ session.table?.kind === 'view' ? 'view' : 'no primary key' }}
    </span>
  </div>
  <div class="section">
    <span>Columns</span>
    <span class="count">{{ rows.length }}</span>
  </div>
  <!-- Columns can be changed without a primary key. Only a view is read only. -->
  <DataGrid
    v-model:selected="selected"
    :columns="HEADERS"
    :rows="rows"
    :editable="session.table?.kind === 'table'"
    @edit="edit"
  />

  <div v-if="session.table?.kind === 'table'" class="keys">
    <PaneResizer size="keysHeight" />
    <div class="keys-body">
      <div class="section">
        <span>Indexes</span>
        <span class="count">{{ session.keys?.indexes.length ?? 0 }}</span>
      </div>
      <table v-if="session.keys?.indexes.length">
        <thead>
          <tr>
            <th>Name</th>
            <th>Columns</th>
            <th>Unique</th>
            <th>Primary</th>
            <th />
          </tr>
        </thead>
        <tbody>
          <tr v-for="index in session.keys.indexes" :key="index.name">
            <td>{{ index.name || '(primary key)' }}</td>
            <td>{{ indexColumnsText(index) }}</td>
            <td>{{ index.unique ? 'YES' : 'NO' }}</td>
            <td>{{ index.primary ? 'YES' : 'NO' }}</td>
            <td>
              <button
                v-if="!index.primary"
                class="icon"
                :title="`Drop index ${index.name}`"
                @click="session.run(dropIndex(index))"
              >
                <Trash2Icon :size="12" />
              </button>
            </td>
          </tr>
        </tbody>
      </table>
      <div class="bar form">
        <input
          v-model="indexName"
          class="key-name mono"
          placeholder="new index name"
          autocapitalize="off"
          spellcheck="false"
          @keydown.enter="addIndex"
        />
        <ColumnPicker v-model="indexColumns" :options="columnNames" placeholder="add column" />
        <label class="check" title="No two rows may have the same values">
          <input v-model="indexUnique" type="checkbox" />
          Unique
        </label>
        <button
          class="primary"
          :disabled="!indexName.trim() || indexColumns.length === 0"
          @click="addIndex"
        >
          Add index
        </button>
      </div>

      <div class="section">
        <span>Foreign keys</span>
        <span class="count">{{ session.keys?.foreignKeys.length ?? 0 }}</span>
      </div>
      <table v-if="session.keys?.foreignKeys.length">
        <thead>
          <tr>
            <th>Name</th>
            <th>Columns</th>
            <th>References</th>
            <th>On delete</th>
            <th>On update</th>
            <th v-if="session.canChangeForeignKeys" />
          </tr>
        </thead>
        <tbody>
          <tr v-for="(key, position) in session.keys.foreignKeys" :key="`${position}-${key.name}`">
            <td>{{ key.name || '(no name)' }}</td>
            <td>{{ key.columns.join(', ') }}</td>
            <td>{{ references(key) }}</td>
            <td>{{ key.onDelete }}</td>
            <td>{{ key.onUpdate }}</td>
            <td v-if="session.canChangeForeignKeys">
              <button
                class="icon"
                :title="`Drop foreign key ${key.name}`"
                @click="session.run(dropForeignKey(key))"
              >
                <Trash2Icon :size="12" />
              </button>
            </td>
          </tr>
        </tbody>
      </table>
      <div v-if="!session.canChangeForeignKeys" class="bar form dim">
        Read only: SQLite cannot add or drop a foreign key of an existing table.
      </div>
      <div v-else class="bar form">
        <input
          v-model="keyName"
          class="key-name mono"
          placeholder="new foreign key name"
          autocapitalize="off"
          spellcheck="false"
          @keydown.enter="addForeignKey"
        />
        <ColumnPicker v-model="keyColumns" :options="columnNames" placeholder="add column" />
        <span class="dim">references</span>
        <select v-model="keyRefTable" title="The table the key points at">
          <option :value="null">table</option>
          <option v-for="table in refTables" :key="tableLabel(table)" :value="table">
            {{ tableLabel(table) }}
          </option>
        </select>
        <ColumnPicker
          v-model="keyRefColumns"
          :options="refTableColumns"
          placeholder="add referenced column"
        />
        <label class="rule">
          On delete
          <select v-model="keyOnDelete">
            <option v-for="action in REFERENTIAL_ACTIONS" :key="action" :value="action">
              {{ action }}
            </option>
          </select>
        </label>
        <label class="rule">
          On update
          <select v-model="keyOnUpdate">
            <option v-for="action in REFERENTIAL_ACTIONS" :key="action" :value="action">
              {{ action }}
            </option>
          </select>
        </label>
        <button class="primary" :disabled="!canAddKey" @click="addForeignKey">
          Add foreign key
        </button>
      </div>
    </div>
  </div>

  <BottomBar :session="session">
    <template v-if="session.table?.kind === 'table'">
      <input
        v-model="newName"
        class="new-name mono"
        placeholder="new column name"
        autocapitalize="off"
        spellcheck="false"
        @keydown.enter="addColumn"
      />
      <input
        v-model="newType"
        class="new-type mono"
        placeholder="type, like VARCHAR(255)"
        autocapitalize="off"
        spellcheck="false"
        @keydown.enter="addColumn"
      />
      <label class="check" title="The column may hold NULL">
        <input v-model="newNullable" type="checkbox" />
        Null
      </label>
      <input
        v-model="newDefault"
        class="new-default mono"
        placeholder="default as SQL, like 'text' or 0"
        autocapitalize="off"
        spellcheck="false"
        @keydown.enter="addColumn"
      />
      <button class="primary" :disabled="!newName.trim() || !newType.trim()" @click="addColumn">
        Add column
      </button>
      <span class="divider" />
      <button
        class="icon"
        title="Drop the selected column"
        :disabled="selected.length !== 1"
        @click="session.run(dropSelected())"
      >
        <MinusIcon :size="13" />
      </button>
    </template>
    <template #end>
      <span v-if="session.table?.kind === 'table'" class="dim">
        Double click a cell to change it
      </span>
    </template>
  </BottomBar>
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

.section {
  height: 24px;
  padding: 0 10px;
  border-bottom: 1px solid var(--border-soft);
}

.new-name {
  width: 140px;
}

/* Below the column grid. The user drags its top edge. The body scrolls on its own. */
.keys {
  position: relative;
  flex: none;
  height: var(--keys-height);
  max-height: 80%;
  border-top: 1px solid var(--border);
  background: var(--bg);
}

.keys-body {
  height: 100%;
  overflow: auto;
}

/* The same look as the DataGrid, like the Users view. */
table {
  border-spacing: 0;
  font-size: 11px;
}

th,
td {
  height: 22px;
  padding: 0 8px;
  text-align: left;
  white-space: nowrap;
}

/* The section title (24px) and the table header stay on top while the body scrolls. */
.keys-body .section {
  position: sticky;
  top: 0;
  z-index: 3;
  background: var(--bg);
}

th {
  position: sticky;
  top: 24px;
  z-index: 2;
  background: var(--header);
  border-right: 1px solid var(--border);
  border-bottom: 1px solid var(--border);
  color: var(--text-2);
  font-weight: 600;
}

td {
  border-right: 1px solid var(--border-soft);
  border-bottom: 1px solid var(--border-soft);
  font-family: var(--font-mono);
  -webkit-user-select: text;
  user-select: text;
}

tbody tr:nth-child(even) {
  background: var(--row-alt);
}

td button.icon {
  height: 18px;
}

/* A form row for a new index or foreign key. */
.bar.form {
  height: auto;
  min-height: 30px;
  flex-wrap: wrap;
  padding: 4px 10px;
  background: var(--bg);
  border-bottom: 1px solid var(--border-soft);
}

.bar select {
  height: 22px;
  padding: 0 4px;
  border-radius: 5px;
  font-size: 11px;
}

.key-name {
  width: 160px;
}

.rule {
  display: inline-flex;
  gap: 4px;
  align-items: center;
  color: var(--text-2);
}

.new-type {
  width: 160px;
}

.new-default {
  width: 190px;
}

.check {
  display: inline-flex;
  gap: 4px;
  align-items: center;
}

.check input {
  width: 13px;
  height: 13px;
  margin: 0;
  padding: 0;
  accent-color: var(--accent);
}
</style>
