<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { confirm } from '@tauri-apps/plugin-dialog'
import {
  Columns3Icon,
  KeyRoundIcon,
  ListTreeIcon,
  LockIcon,
  MinusIcon,
  TableIcon,
  Trash2Icon,
} from 'lucide-vue-next'
import { DEFAULT_REFERENTIAL_ACTION, REFERENTIAL_ACTIONS } from '../constants/keys'
import { SECTION_MIN_HEIGHT } from '../constants/layout'
import { LAYOUT_SERVICE, injectRequired } from '../services/keys'
import type { Session } from '../services/Session'
import { StructureRows } from '../services/StructureRows'
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
import SectionHeading from './SectionHeading.vue'
import SectionResizer from './SectionResizer.vue'

// Key, Extra and the details cannot be changed here. Default can be set to "no default" with NULL.
// The order matches the cells of StructureRows.
const HEADERS: GridColumn[] = [
  { name: 'Field' },
  { name: 'Type' },
  { name: 'Charset', readOnly: true },
  { name: 'Collation', readOnly: true },
  { name: 'Null' },
  { name: 'Key', readOnly: true },
  { name: 'Default', nullable: true },
  { name: 'Extra', readOnly: true },
  // The arrow opens the referenced table.
  { name: 'Foreign key', readOnly: true, link: true },
  { name: 'Comment', readOnly: true },
]
const structureRows = new StructureRows()

const props = defineProps<{ session: Session }>()
const layout = injectRequired(LAYOUT_SERVICE)
const rows = computed(gridRows)
const rowCount = computed(describeRowCount)
const selected = ref<number[]>([])

/**
 * A dragged section has a fixed height. Without one it is as tall as its content.
 * It still shrinks when the window is too short, down to heading + header + one row.
 */
function sectionStyle(height: number | null): Record<string, string> {
  return height === null ? {} : { flex: `0 1 ${height}px`, minHeight: `${SECTION_MIN_HEIGHT}px` }
}

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
function gridRows(): Row[] {
  return structureRows.rows(props.session.page?.columns ?? [], props.session.keys)
}

/** The arrow of a Foreign key cell opens the referenced table. */
function openReference(rowIndex: number): void {
  const { session } = props
  const column = session.page?.columns[rowIndex]
  if (!column || !session.table) {
    return
  }
  const target = structureRows.target(session.keys, column.name, session.table, session.tables)
  if (target) {
    session.run(session.selectTable(target))
  } else {
    session.status = 'The referenced table is not in the table list'
  }
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
  <!-- Three sections with the same build: a heading, then a body that scrolls on its own. -->
  <div class="sections">
    <section
      class="part"
      :class="{ open: layout.columnsOpen }"
      :style="layout.columnsOpen ? sectionStyle(layout.columnsHeight) : {}"
    >
      <SectionHeading
        title="Columns"
        :count="rows.length"
        :open="layout.columnsOpen"
        :icon="Columns3Icon"
        icon-color="var(--folder-blue)"
        @toggle="layout.toggleSection('columnsOpen')"
      />
      <!-- Columns can be changed without a primary key. Only a view is read only. -->
      <DataGrid
        v-if="layout.columnsOpen"
        v-model:selected="selected"
        :columns="HEADERS"
        :rows="rows"
        :editable="session.table?.kind === 'table'"
        @edit="edit"
        @link="openReference"
      />
      <SectionResizer v-if="layout.columnsOpen" size="columnsHeight" />
    </section>

    <!-- A view has columns only. -->
    <template v-if="session.table?.kind === 'table'">
      <section
        class="part"
        :class="{ open: layout.indexesOpen }"
        :style="layout.indexesOpen ? sectionStyle(layout.indexesHeight) : {}"
      >
        <SectionHeading
          title="Indexes"
          :count="session.keys?.indexes.length ?? 0"
          :open="layout.indexesOpen"
          :icon="ListTreeIcon"
          icon-color="var(--folder-green)"
          split
          @toggle="layout.toggleSection('indexesOpen')"
        />
        <div v-if="layout.indexesOpen" class="body">
          <table v-if="session.keys">
            <thead>
              <tr>
                <th>Name</th>
                <th>Columns</th>
                <th>Unique</th>
                <th>Primary</th>
                <th />
                <th class="filler" />
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
                    class="icon danger"
                    :title="`Drop index ${index.name}`"
                    @click="session.run(dropIndex(index))"
                  >
                    <Trash2Icon :size="12" />
                  </button>
                </td>
                <td class="filler" />
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
        </div>
        <SectionResizer v-if="layout.indexesOpen" size="indexesHeight" />
      </section>

      <section
        class="part"
        :class="{ open: layout.foreignKeysOpen }"
        :style="layout.foreignKeysOpen ? sectionStyle(layout.foreignKeysHeight) : {}"
      >
        <SectionHeading
          title="Foreign keys"
          :count="session.keys?.foreignKeys.length ?? 0"
          :open="layout.foreignKeysOpen"
          :icon="KeyRoundIcon"
          icon-color="var(--folder-orange)"
          split
          @toggle="layout.toggleSection('foreignKeysOpen')"
        />
        <div v-if="layout.foreignKeysOpen" class="body">
          <table v-if="session.keys">
            <thead>
              <tr>
                <th>Name</th>
                <th>Columns</th>
                <th>References</th>
                <th>On delete</th>
                <th>On update</th>
                <th v-if="session.canChangeForeignKeys" />
                <th class="filler" />
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="(key, position) in session.keys.foreignKeys"
                :key="`${position}-${key.name}`"
              >
                <td>{{ key.name || '(no name)' }}</td>
                <td>{{ key.columns.join(', ') }}</td>
                <td>{{ references(key) }}</td>
                <td>{{ key.onDelete }}</td>
                <td>{{ key.onUpdate }}</td>
                <td v-if="session.canChangeForeignKeys">
                  <button
                    class="icon danger"
                    :title="`Drop foreign key ${key.name}`"
                    @click="session.run(dropForeignKey(key))"
                  >
                    <Trash2Icon :size="12" />
                  </button>
                </td>
                <td class="filler" />
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
        <SectionResizer v-if="layout.foreignKeysOpen" size="foreignKeysHeight" />
      </section>
    </template>
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
        class="icon danger"
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

.new-name {
  width: 140px;
}

/* Holds the three sections. Takes the space down to the bottom bar. */
.sections {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-height: 0;
  /* Only when the window is too short for the smallest sections. */
  overflow: auto;
}

/*
 * One section. Not named "section": the global .section in style.css is the
 * uppercase sidebar title, and it would center and inset the tables here.
 * A closed section is only its heading.
 */
.part {
  position: relative;
  display: flex;
  flex: none;
  flex-direction: column;
}

/*
 * An open section is as tall as its content. Free space stays at the bottom.
 * When the sections do not fit, they all shrink and each body scrolls on its own.
 * Each keeps room for the heading, the header and 3 rows.
 */
.part.open {
  flex: 0 1 auto;
  min-height: 127px;
}

/* The last section keeps its handle inside, so the handle does not make the list scroll. */
.part:last-child > .resizer {
  bottom: 0;
}

/* The body of the Indexes and Foreign keys sections. The DataGrid is the body of Columns. */
.body {
  flex: 1;
  min-height: 0;
  overflow: auto;
  background: var(--bg);
}

/* The same look as the DataGrid, like the Users view. */
table {
  min-width: 100%;
  border-spacing: 0;
  font-size: 11px;
}

th,
td {
  height: 23px;
  padding: 0 8px;
  text-align: left;
  white-space: nowrap;
}

/* The table header stays on top while the body scrolls. */
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

/* A short column line that stops 7px from the top and bottom, like the DataGrid header. */
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

/* An empty last column that takes the free width, like the DataGrid. The other columns keep their content width. */
.filler {
  width: 100%;
  padding: 0;
}

th.filler::after {
  content: none;
}

td {
  border-right: 1px solid var(--border-soft);
  font-family: var(--font-mono);
  -webkit-user-select: text;
  user-select: text;
}

/* The last row gets a bottom line, like the DataGrid. */
tbody tr:last-child td {
  border-bottom: 1px solid var(--border-soft);
}

tbody tr:nth-child(even) {
  background: var(--row-alt);
}

td.filler {
  border-right: 0;
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
  /* No line here. The heading of the next section draws the only separator. */
  border-bottom: 0;
  background: var(--bg);
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
