<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, useTemplateRef, watch } from 'vue'
import { confirm } from '@tauri-apps/plugin-dialog'
import { CircleAlertIcon, CircleCheckIcon, PlayIcon } from 'lucide-vue-next'
import type { Session } from '../services/Session'
import { SqlEditor } from '../services/SqlEditor'
import { SqlRisk } from '../services/SqlRisk'
import type { GridColumn } from '../types'
import DataGrid from './DataGrid.vue'

const props = defineProps<{ session: Session }>()

const host = useTemplateRef<HTMLDivElement>('host')
const columns = computed(resultColumns)
// Not a ref: Vue must not wrap the CodeMirror objects.
let editor: SqlEditor | null = null

function resultColumns(): GridColumn[] {
  const columns: GridColumn[] = []
  for (const name of props.session.result?.columns ?? []) {
    columns.push({ name })
  }
  return columns
}

/** Runs the selected text. Runs everything when nothing is selected. */
function run(): void {
  const selection = editor?.selectedText() ?? ''
  const sql = selection.trim() ? selection : props.session.sql
  if (sql.trim()) {
    props.session.run(runChecked(sql))
  }
}

/** Asks first when the SQL can change or remove a lot of data. */
async function runChecked(sql: string): Promise<void> {
  const warning = new SqlRisk(props.session.config?.driver).message(sql)
  if (warning && !(await confirm(warning, { title: 'Run risky SQL?', kind: 'warning' }))) {
    return
  }
  await props.session.runQuery(sql)
}

function saveText(text: string): void {
  props.session.sql = text
}

function currentSql(): string {
  return props.session.sql
}

function showSql(text: string): void {
  editor?.setText(text)
}

/** The driver and the table list. The editor needs both for its language. */
function schemaSource(): [Session['tables'], string | undefined] {
  return [props.session.tables, props.session.config?.driver]
}

function updateSchema(): void {
  editor?.setSchema(props.session.config?.driver, props.session.tables)
}

/** The editor is built one time here. Later changes go through its methods. */
function createEditor(): void {
  if (!host.value) {
    return
  }
  editor = new SqlEditor({
    parent: host.value,
    text: props.session.sql,
    onChange: saveText,
    onRun: run,
  })
  updateSchema()
}

function destroyEditor(): void {
  editor?.destroy()
  editor = null
}

onMounted(createEditor)
onBeforeUnmount(destroyEditor)
watch(currentSql, showSql)
watch(schemaSource, updateSchema)
</script>

<template>
  <div ref="host" class="editor" />
  <div class="bar tools">
    <button
      class="primary"
      title="Run the selected text, or everything"
      :disabled="session.busy"
      @click="run"
    >
      <PlayIcon :size="11" />
      Run
      <span class="hint">⌘↵</span>
    </button>
    <span class="dim">Select text to run only that part.</span>
  </div>
  <DataGrid v-if="session.result" :columns="columns" :rows="session.result.rows" />
  <div v-else class="empty">No result</div>
  <div class="bar bottom result">
    <template v-if="session.error">
      <CircleAlertIcon class="error" :size="13" />
      <span class="message error" :title="session.error">{{ session.error }}</span>
    </template>
    <span v-else-if="session.busy" class="dim">Working…</span>
    <template v-else-if="session.status">
      <CircleCheckIcon class="ok" :size="13" />
      <span class="message">{{ session.status }}</span>
    </template>
  </div>
</template>

<style scoped>
/* The wrapper can be resized. The editor fills it and scrolls inside. */
.editor {
  flex: none;
  height: 220px;
  min-height: 40px;
  max-height: 70%;
  overflow: hidden;
  resize: vertical;
  -webkit-user-select: text;
  user-select: text;
}

.tools {
  border-top: 1px solid var(--border);
}

.result {
  gap: 8px;
  padding: 0 10px;
}

.ok {
  color: var(--success);
}
</style>
