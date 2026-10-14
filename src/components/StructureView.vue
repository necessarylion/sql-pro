<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { confirm } from '@tauri-apps/plugin-dialog'
import { LockIcon, MinusIcon, TableIcon } from 'lucide-vue-next'
import type { Session } from '../services/Session'
import { TableDraft } from '../services/TableDraft'
import type { ColumnSpec, GridColumn, Row, TablePage } from '../types'
import BottomBar from './BottomBar.vue'
import DataGrid from './DataGrid.vue'

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

// Row numbers mean other columns after a reload. Drop the selection.
watch(sessionPage, clearSelection)

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
