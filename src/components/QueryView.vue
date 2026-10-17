<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, useTemplateRef, watch } from 'vue'
import { confirm } from '@tauri-apps/plugin-dialog'
import {
  CircleAlertIcon,
  CircleCheckIcon,
  DownloadIcon,
  HistoryIcon,
  ListTreeIcon,
  PlayIcon,
  SaveIcon,
  WandSparklesIcon,
  XIcon,
} from 'lucide-vue-next'
import { QUERY_HISTORY, SAVED_QUERIES, SHORTCUTS, injectRequired } from '../services/keys'
import type { Session } from '../services/Session'
import { SqlEditor } from '../services/SqlEditor'
import { SqlFormat } from '../services/SqlFormat'
import { SqlRisk } from '../services/SqlRisk'
import type { ColumnInfo, GridColumn, ResultSet, Row, TableRef } from '../types'
import DataGrid from './DataGrid.vue'
import ExportRowsMenu from './ExportRowsMenu.vue'
import PaneResizer from './PaneResizer.vue'
import QueryHistoryList from './QueryHistoryList.vue'
import SavedQueryDialog from './SavedQueryDialog.vue'
import SavedQueryList from './SavedQueryList.vue'

const props = defineProps<{ session: Session }>()
const shortcuts = injectRequired(SHORTCUTS)
const history = injectRequired(QUERY_HISTORY)
const saved = injectRequired(SAVED_QUERIES)
const savedDialog = useTemplateRef<InstanceType<typeof SavedQueryDialog>>('savedDialog')
// The saved queries of this tab are the ones of its server.
const savedKey = computed(savedQueriesKey)

const host = useTemplateRef<HTMLDivElement>('host')
const columns = computed(resultColumns)
const selected = ref<number[]>([])
const grid = useTemplateRef<InstanceType<typeof DataGrid>>('grid')
// Where the export menu is open. null = closed.
const menu = ref<{ x: number; y: number } | null>(null)
const exportRows = computed(rowsToExport)
// True when the history panel is open on the right of the result, like the row detail panel.
const showHistory = ref(false)
// Not a ref: Vue must not wrap the CodeMirror objects.
let editor: SqlEditor | null = null

function savedQueriesKey(): string {
  return saved.key(props.session.config)
}

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

/** The selected text, or everything when nothing is selected. */
function pickedSql(): string {
  const selection = editor?.selectedText() ?? ''
  return selection.trim() ? selection : props.session.sql
}

/** Runs the selected text. Runs everything when nothing is selected. */
function run(): void {
  const sql = pickedSql()
  if (sql.trim()) {
    props.session.run(runChecked(sql))
  }
}

/** Shows the query plan of the selected text, or of everything. Nothing runs. */
function explain(): void {
  const sql = pickedSql()
  if (sql.trim()) {
    props.session.run(props.session.explain(sql))
  }
}

/** Formats the selected text, or everything. Bad SQL stays as it is and shows a short error. */
function formatSql(): void {
  try {
    editor?.format(new SqlFormat(props.session.config?.driver))
    props.session.error = ''
  } catch (error) {
    // The parser message has more lines about the dialect. The first line says where it failed.
    props.session.error = `Cannot format: ${String(error).split('\n')[0]}`
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

/** Puts SQL from the history into the editor and closes the panel. */
function useHistory(sql: string): void {
  props.session.sql = sql
  showHistory.value = false
}

function closeHistory(): void {
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

function listColumns(table: TableRef): Promise<ColumnInfo[]> {
  return props.session.listColumns(table)
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
    onFormat: formatSql,
    columns: listColumns,
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
  <!-- Escape anywhere below the editor closes the history panel. -->
  <div class="below" @keydown.esc="closeHistory">
    <div class="bar tools">
      <button
        title="Show the saved queries and the SQL that was run before"
        :class="{ active: showHistory }"
        :aria-pressed="showHistory"
        @click="showHistory = !showHistory"
      >
        <HistoryIcon :size="11" />
        History
      </button>
      <button
        title="Save the SQL in the editor under a name"
        :disabled="!session.sql.trim()"
        @click="savedDialog?.save(session.sql)"
      >
        <SaveIcon :size="11" />
        Save
      </button>
      <button
        :title="`Format the selected text, or everything (${shortcuts.hint('F', true)})`"
        @click="formatSql"
      >
        <WandSparklesIcon :size="11" />
        Format
      </button>
      <span class="spacer" />
      <span class="dim">Select text to run only that part.</span>
      <!-- SQL Server: the driver cannot read a SHOWPLAN result for sure. -->
      <button
        v-if="session.config?.driver !== 'mssql'"
        title="Show the query plan of one statement. It does not run."
        :disabled="session.busy"
        @click="explain"
      >
        <ListTreeIcon :size="11" />
        Explain
      </button>
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
    <div class="body">
      <DataGrid
        v-if="session.result"
        ref="grid"
        v-model:selected="selected"
        :columns="columns"
        :rows="session.result.rows"
        @menu="openMenu"
      />
      <div v-else class="empty">No result</div>
      <!-- tabindex: a click inside takes the focus, so Escape reaches the handler above. -->
      <aside v-if="showHistory" class="queries" tabindex="-1">
        <PaneResizer size="detailWidth" />
        <div class="head">
          <span class="title">History</span>
          <span class="spacer" />
          <button class="icon" title="Hide the history (Esc)" @click="closeHistory">
            <XIcon :size="12" />
          </button>
        </div>
        <SavedQueryList
          :connection-key="savedKey"
          @pick="useHistory"
          @rename="savedDialog?.rename($event)"
        />
        <QueryHistoryList @pick="useHistory" />
      </aside>
    </div>
    <ExportRowsMenu
      v-if="menu && session.result"
      :session="session"
      :x="menu.x"
      :y="menu.y"
      :columns="session.result.columns"
      :rows="exportRows"
      :table="null"
      copies
      @close="closeMenu"
      @copy="grid?.copyRows()"
    />
    <SavedQueryDialog ref="savedDialog" :connection-key="savedKey" />
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
        v-if="session.result"
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

/* The result and the history panel, side by side. Like the Content view. */
.body {
  display: flex;
  flex: 1;
  min-height: 0;
}

.body > .grid {
  min-width: 0;
}

/* The history panel on the right: the saved queries above the history. Same look as RowDetail.vue. */
.queries {
  position: relative;
  /* Above the grid header (z-index 2), below menus and dialogs (100). */
  z-index: 3;
  box-shadow: var(--panel-shadow);
  display: flex;
  flex: none;
  flex-direction: column;
  width: var(--detail-width);
  /* The grid keeps some room, also with a wide saved width. */
  max-width: calc(100% - 160px);
  min-height: 0;
  background: var(--bg);
  border-left: 1px solid var(--border);
  outline: none;
}

.head {
  display: flex;
  flex: none;
  gap: 6px;
  align-items: center;
  /* Same height as the grid header next to it, so the bottom borders line up. */
  height: 28px;
  padding: 0 4px 0 10px;
  background: var(--detail-header);
  border-bottom: 1px solid var(--border);
  font-size: 11px;
  white-space: nowrap;
}

.head .title {
  color: var(--text-2);
  font-weight: 600;
}

.head .spacer {
  flex: 1;
}

.head button.icon {
  width: 18px;
  height: 18px;
}

/* The History button while the list is open. */
.tools button.active {
  border-color: var(--accent);
  color: var(--accent-text);
}
</style>
