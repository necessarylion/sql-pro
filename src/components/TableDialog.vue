<script setup lang="ts">
import { nextTick, reactive, useTemplateRef } from 'vue'
import { PlusIcon, XIcon } from 'lucide-vue-next'
import type { Session } from '../services/Session'
import { TableDraft } from '../services/TableDraft'
import type { ColumnDraft } from '../services/TableDraft'
import type { TableRef } from '../types'

// The dialog to create a table, or to rename one.
const props = defineProps<{ session: Session }>()
const dialog = useTemplateRef<HTMLDialogElement>('dialog')
const nameInput = useTemplateRef<HTMLInputElement>('nameInput')

// `table` is the table that is renamed. null means a new table.
const form = reactive({
  table: null as TableRef | null,
  name: '',
  columns: [] as ColumnDraft[],
  problem: '',
})

/** Opens the dialog. Pass a table to rename it. Pass null for a new table. */
async function open(table: TableRef | null): Promise<void> {
  form.table = table
  form.name = table?.name ?? ''
  form.columns = [TableDraft.idColumn()]
  form.problem = ''
  // The native dialog closes on Escape by itself.
  dialog.value?.showModal()
  // Select the old name, so typing replaces it.
  await nextTick()
  nameInput.value?.select()
}

function close(): void {
  dialog.value?.close()
}

function addColumn(): void {
  form.columns.push(TableDraft.emptyColumn())
}

function removeColumn(index: number): void {
  form.columns.splice(index, 1)
  TableDraft.keyChanged(form.columns)
}

function keyChanged(): void {
  TableDraft.keyChanged(form.columns)
}

/** The dialog stays open on an error and shows it. */
async function submit(): Promise<void> {
  const session = props.session
  const work = form.table
    ? session.renameTable(form.table, form.name)
    : session.createTable(form.name, form.columns)
  if (await session.run(work)) {
    close()
  } else {
    form.problem = session.error
  }
}

defineExpose({ open })
</script>

<template>
  <dialog ref="dialog" class="table-dialog" :class="{ wide: !form.table }">
    <form class="panel" @submit.prevent="submit">
      <h2>{{ form.table ? `Rename table "${form.table.name}"` : 'New table' }}</h2>
      <div class="row">
        <label for="table-name">Name</label>
        <input
          id="table-name"
          ref="nameInput"
          v-model="form.name"
          class="mono"
          required
          autofocus
          autocapitalize="off"
          spellcheck="false"
        />
      </div>
      <template v-if="!form.table">
        <div class="columns">
          <div v-for="(column, index) in form.columns" :key="index" class="column">
            <input
              v-model="column.name"
              class="mono"
              placeholder="column name"
              autocapitalize="off"
              spellcheck="false"
            />
            <input
              v-model="column.dataType"
              class="mono"
              placeholder="type, like VARCHAR(255)"
              autocapitalize="off"
              spellcheck="false"
            />
            <label class="check" title="The column may hold NULL">
              <input v-model="column.nullable" type="checkbox" :disabled="column.primaryKey" />
              Null
            </label>
            <input
              v-model="column.default"
              class="mono"
              placeholder="default as SQL, like 'text' or 0"
              autocapitalize="off"
              spellcheck="false"
            />
            <label class="check" title="The column is part of the primary key">
              <input v-model="column.primaryKey" type="checkbox" @change="keyChanged" />
              Key
            </label>
            <label
              class="check"
              title="Auto increment: the database numbers the rows itself. Needs an integer column that is the whole primary key."
            >
              <input
                v-model="column.autoIncrement"
                type="checkbox"
                :disabled="!column.primaryKey || TableDraft.keyCount(form.columns) !== 1"
              />
              Auto
            </label>
            <button
              type="button"
              class="icon"
              title="Remove this column"
              @click="removeColumn(index)"
            >
              <XIcon :size="12" />
            </button>
          </div>
        </div>
        <button type="button" class="add" @click="addColumn">
          <PlusIcon :size="12" />
          Add column
        </button>
      </template>
      <p v-if="form.problem" class="error">{{ form.problem }}</p>
      <div class="actions">
        <button type="button" @click="close">Cancel</button>
        <button type="submit" class="primary" :disabled="session.busy">
          {{ form.table ? 'Rename' : 'Create table' }}
        </button>
      </div>
    </form>
  </dialog>
</template>

<style scoped>
.table-dialog {
  width: 320px;
  max-width: calc(100vw - 32px);
  padding: 0;
  background: var(--toolbar);
  border: 1px solid var(--border);
  border-radius: 10px;
  box-shadow: 0 12px 32px #00000040;
  color: var(--text);
}

.table-dialog.wide {
  width: 720px;
}

.table-dialog::backdrop {
  background: #00000040;
}

.panel {
  display: flex;
  flex-direction: column;
  gap: 12px;
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

.row {
  display: flex;
  gap: 10px;
  align-items: center;
}

.row label {
  flex: none;
  color: var(--text-2);
}

.row input {
  flex: 1;
  min-width: 0;
}

/* Many columns scroll inside the dialog. The buttons stay in view. */
.columns {
  display: flex;
  flex-direction: column;
  gap: 6px;
  max-height: 50vh;
  overflow-y: auto;
}

.column {
  display: grid;
  grid-template-columns: 3fr 3fr auto 4fr auto auto 22px;
  gap: 8px;
  align-items: center;
}

.column input:not([type='checkbox']) {
  min-width: 0;
}

.check {
  display: inline-flex;
  gap: 4px;
  align-items: center;
  font-size: 11px;
}

.check input {
  width: 13px;
  height: 13px;
  margin: 0;
  padding: 0;
  accent-color: var(--accent);
}

.add {
  align-self: flex-start;
  height: 22px;
  padding: 0 8px;
  font-size: 11px;
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
