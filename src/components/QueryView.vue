<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, useTemplateRef, watch } from 'vue'
import { confirm } from '@tauri-apps/plugin-dialog'
import {
  CircleAlertIcon,
  CircleCheckIcon,
  DownloadIcon,
  HistoryIcon,
  PlayIcon,
} from 'lucide-vue-next'
import { QUERY_HISTORY, SHORTCUTS, injectRequired } from '../services/keys'
import type { Session } from '../services/Session'
import { SqlEditor } from '../services/SqlEditor'
import { SqlRisk } from '../services/SqlRisk'
import type { GridColumn, ResultSet, Row } from '../types'
import DataGrid from './DataGrid.vue'
import ExportRowsMenu from './ExportRowsMenu.vue'
import PaneResizer from './PaneResizer.vue'
import QueryHistoryList from './QueryHistoryList.vue'

const props = defineProps<{ session: Session }>()
const shortcuts = injectRequired(SHORTCUTS)
const history = injectRequired(QUERY_HISTORY)

const host = useTemplateRef<HTMLDivElement>('host')
const columns = computed(resultColumns)
const selected = ref<number[]>([])
// Where the export menu is open. null = closed.
const menu = ref<{ x: number; y: number } | null>(null)
const exportRows = computed(rowsToExport)
// True when the history list is shown in the place of the result.
const showHistory = ref(false)
// Not a ref: Vue must not wrap the CodeMirror objects.
let editor: SqlEditor | null = null

function resultColumns(): GridColumn[] {
  const columns: GridColumn[] = []
  for (const name of props.session.result?.columns ?? []) {
    columns.push({ name })
  }
  return columns
}

function sessionResult(): ResultSet | null {
  return props.session.result
}

function clearSelection(): void {
  selected.value = []
}

/** The selected result rows, or all of them when none is selected. */
function rowsToExport(): Row[] {
  return props.session.pickRows(props.session.result?.rows ?? [], selected.value)
}

function openMenu(event: MouseEvent): void {
  menu.value = { x: event.clientX, y: event.clientY }
}

function closeMenu(): void {
  menu.value = null
}

/** Runs the selected text. Runs everything when nothing is selected. */
function run(): void {
  const selection = editor?.selectedText() ?? ''
  const sql = selection.trim() ? selection : props.session.sql
  if (sql.trim()) {
    // Show the result, not the history.
    showHistory.value = false
    props.session.run(runChecked(sql))
  }
}

/** Asks first when the SQL can change or remove a lot of data. */
async function runChecked(sql: string): Promise<void> {
  const warning = new SqlRisk(props.session.config?.driver).message(sql)
  if (warning && !(await confirm(warning, { title: 'Run risky SQL?', kind: 'warning' }))) {
    return
  }
  // Saved before the run. So SQL that fails is kept too.
  history.add(sql, props.session.title)
  await props.session.runQuery(sql)
}

/** Puts SQL from the history into the editor and closes the list. */
function useHistory(sql: string): void {
  props.session.sql = sql
  showHistory.value = false
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
// Row numbers mean other rows in a new result. Drop the selection.
watch(sessionResult, clearSelection)
</script>

<template>
  <div class="editor">
    <div ref="host" class="host" />
    <PaneResizer size="editorHeight" />
  </div>
  <div class="below">
    <div class="bar tools">
      <button
        title="Show the SQL that was run before"
        :class="{ active: showHistory }"
        :aria-pressed="showHistory"
        @click="showHistory = !showHistory"
      >
        <HistoryIcon :size="11" />
        History
      </button>
      <span class="spacer" />
      <span class="dim">Select text to run only that part.</span>
      <button
        class="primary"
        title="Run the selected text, or everything"
        :disabled="session.busy"
        @click="run"
      >
        <PlayIcon :size="11" />
        Run
        <span class="hint">{{ shortcuts.hint('↵') }}</span>
      </button>
    </div>
    <QueryHistoryList v-if="showHistory" @pick="useHistory" />
    <DataGrid
      v-else-if="session.result"
      v-model:selected="selected"
      :columns="columns"
      :rows="session.result.rows"
      @menu="openMenu"
    />
    <div v-else class="empty">No result</div>
    <ExportRowsMenu
      v-if="menu && session.result"
      :session="session"
      :x="menu.x"
      :y="menu.y"
      :columns="session.result.columns"
      :rows="exportRows"
      :table="null"
      @close="closeMenu"
    />
    <div class="bar bottom result">
      <template v-if="session.error">
        <CircleAlertIcon class="error" :size="13" />
        <span class="message error" :title="session.error">{{ session.error }}</span>
      </template>
      <template v-else-if="session.busy">
        <span class="dim">Working…</span>
        <button
          v-if="session.cancellable"
          title="Stop the running query (Esc)"
          :disabled="session.cancelling"
          @click="session.cancel()"
        >
          {{ session.cancelling ? 'Cancelling…' : 'Cancel' }}
        </button>
      </template>
      <template v-else-if="session.status">
        <CircleCheckIcon class="ok" :size="13" />
        <span class="message">{{ session.status }}</span>
      </template>
      <button
        v-if="session.result && !showHistory"
        class="icon export"
        :title="selected.length > 0 ? 'Export selected rows' : 'Export all result rows'"
        :disabled="exportRows.length === 0"
        @click="openMenu"
      >
        <DownloadIcon :size="13" />
      </button>
    </div>
  </div>
</template>

<style scoped>
/*
  The editor has the chosen height. It gets smaller when the window is too small,
  because only the editor can shrink. The area below never goes under its 120px.
*/
.editor {
  position: relative;
  flex: 0 1 var(--editor-height);
  min-height: 80px;
  border-bottom: 1px solid var(--border);
}

/* CodeMirror fills this box and scrolls inside it. */
.host {
  position: absolute;
  inset: 0;
  overflow: hidden;
  -webkit-user-select: text;
  user-select: text;
}

.below {
  display: flex;
  flex: 1 0 120px;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
}

.result {
  gap: 8px;
  padding: 0 10px;
}

.ok {
  color: var(--success);
}

/* Stays on the right edge, also when there is no status text. */
.export {
  margin-left: auto;
}

/* The hint gives way first in a narrow window, so the Run button stays visible. */
.tools .dim {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* The History button while the list is open. */
.tools button.active {
  border-color: var(--accent);
  color: var(--accent-text);
}
</style>
