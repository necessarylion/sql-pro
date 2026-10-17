<script setup lang="ts">
import { computed, onMounted, reactive, ref, useTemplateRef, watch } from 'vue'
import { CopyIcon, GaugeIcon, PlayIcon, RefreshCwIcon, SparklesIcon } from 'lucide-vue-next'
import {
  QUERY_STATS_DAY_CHOICES,
  QUERY_STATS_MAX_DAYS,
  QUERY_STATS_MIN_DAYS,
} from '../constants/queryStats'
import { SHORTCUTS, injectRequired } from '../services/keys'
import { QueryStatList } from '../services/QueryStatList'
import { ExplainQueryPrompt } from '../services/ExplainQueryPrompt'
import type { Session } from '../services/Session'
import type { QueryStatsOption, QueryStatsStep } from '../constants/queryStats'
import type { QueryStat, Row, SnippetOutput } from '../types'
import BottomBar from './BottomBar.vue'
import ContextMenu from './ContextMenu.vue'
import DataGrid from './DataGrid.vue'

const props = defineProps<{ session: Session }>()
const list = reactive(new QueryStatList())
const columns = QueryStatList.columns()
const shown = computed(showStats)
const rows = computed(gridRows)
const selected = ref<number[]>([])
const shortcuts = injectRequired(SHORTCUTS)
const grid = useTemplateRef<InstanceType<typeof DataGrid>>('grid')
const count = computed(describeCount)
/** PostgreSQL keeps no last run time, so it cannot filter by days. */
const hasDays = computed(keepsLastRun)
const picked = computed(lastSelected)
/** The setup steps for this kind of server, shown when the stats are off. */
const guide = computed(setupSteps)
// Where the row menu is open. null = closed.
const menu = ref<{ x: number; y: number } | null>(null)

// The stats change all the time. So load them each time the view opens.
onMounted(reload)
// A new sort, filter or load moves the rows. The old selection would point to other rows.
watch(shown, clearSelection)

function sortColumn(name: string): void {
  list.sortByName(name)
}

function reload(): void {
  props.session.run(props.session.loadQueryStats())
}

function setupSteps(): QueryStatsStep[] {
  const config = props.session.config
  return config
    ? QueryStatList.guide(config.driver, config.user, props.session.queryStatsAccount)
    : []
}

/** A snippet as the guide shows it, with its output once it ran. */
interface ShownSnippet extends QueryStatsOption {
  output?: SnippetOutput
  outputText: string | null
}

/** The main snippet of a step (no extra text), then its options. */
function snippetsOf(step: QueryStatsStep): ShownSnippet[] {
  const snippets: QueryStatsOption[] = []
  if (step.code !== undefined) {
    snippets.push({ text: '', code: step.code, run: step.run })
  }
  const shown: ShownSnippet[] = []
  for (const snippet of snippets.concat(step.options ?? [])) {
    const output = props.session.snippetOutputs[snippet.code]
    shown.push({ ...snippet, output, outputText: output ? QueryStatList.outputText(output) : null })
  }
  return shown
}

/** Only copies. A snippet runs only from its Run button, after OK in the warning. */
async function copySnippet(code: string): Promise<void> {
  await navigator.clipboard.writeText(code)
  props.session.status = 'Copied to the clipboard'
}

function clearSelection(): void {
  selected.value = []
}

function showStats(): QueryStat[] {
  return list.visible(props.session.queryStats)
}

function gridRows(): Row[] {
  const rows: Row[] = []
  for (const stat of shown.value) {
    rows.push(QueryStatList.row(stat))
  }
  return rows
}

function keepsLastRun(): boolean {
  return props.session.config?.driver !== 'postgres'
}

function describeCount(): string {
  const all = props.session.queryStats.length
  const queries = `${all} ${all === 1 ? 'query' : 'queries'}`
  return shown.value.length === all ? queries : `${shown.value.length} of ${queries}`
}

/** The last selected row. The AI and the Query editor act on it. */
function lastSelected(): QueryStat | null {
  const index = selected.value.at(-1)
  return index === undefined ? null : (shown.value[index] ?? null)
}

/** Opens the AI chat with a question about the picked query. It always runs read-only. */
function askAi(): void {
  const stat = picked.value
  const config = props.session.config
  if (stat && config) {
    props.session.run(
      props.session.askAiReadOnly(ExplainQueryPrompt.build(stat, config.driver, config.database)),
    )
  }
}

function openPicked(): void {
  if (picked.value) {
    props.session.openInQuery(picked.value.query)
  }
}

/** The grid has already selected the clicked row. */
function openMenu(event: MouseEvent): void {
  menu.value = { x: event.clientX, y: event.clientY }
}

function closeMenu(): void {
  menu.value = null
}

/**
 * A double-click on a row opens its SQL in the Query editor. The grid is read-only, so it
 * has no other use for it. The press before it already selected the row.
 */
function openRow(event: MouseEvent): void {
  if ((event.target as HTMLElement).closest('tbody tr')) {
    openPicked()
  }
}
</script>

<template>
  <div class="bar info">
    <GaugeIcon class="glyph" :size="13" />
    <span class="name">Query stats</span>
    <span class="dim">{{ count }}</span>
    <input v-model="list.filter" class="filter" placeholder="Filter queries" spellcheck="false" />
    <span class="spacer" />
    <label v-if="hasDays" class="dim" title="Only queries that ran in this many days">
      Last
      <input
        v-model.number="session.queryStatsDays"
        class="days"
        type="number"
        list="query-stats-days"
        :min="QUERY_STATS_MIN_DAYS"
        :max="QUERY_STATS_MAX_DAYS"
        step="1"
        @change="reload"
      />
      days
      <datalist id="query-stats-days">
        <option v-for="days in QUERY_STATS_DAY_CHOICES" :key="days" :value="days" />
      </datalist>
    </label>
    <span v-else class="dim">Since the last stats reset</span>
    <button
      class="icon"
      :title="
        picked
          ? 'Explain this query with AI. It only reads.'
          : 'Select a query to explain it with AI'
      "
      :disabled="!picked"
      @click="askAi"
    >
      <SparklesIcon :size="13" />
    </button>
    <button class="icon" title="Refresh query stats" @click="reload">
      <RefreshCwIcon :size="13" />
    </button>
  </div>
  <div v-if="!session.config?.database" class="empty">Select a database to see its query stats</div>
  <div v-else-if="session.queryStatsSetup" class="setup">
    <div class="setup-panel">
      <p class="setup-title">Query stats are not turned on, or your account may not read them.</p>
      <ol>
        <li v-for="(step, index) in guide" :key="index">
          <span>{{ step.text }}</span>
          <template v-for="snippet in snippetsOf(step)" :key="snippet.code">
            <span v-if="snippet.text" class="option">{{ snippet.text }}</span>
            <div class="snippet">
              <pre>{{ snippet.code }}</pre>
              <button class="icon" title="Copy" @click="session.run(copySnippet(snippet.code))">
                <CopyIcon :size="12" />
              </button>
              <button
                v-if="snippet.run"
                class="icon run"
                title="Run on the server. It asks first."
                @click="session.run(session.runQueryStatsSnippet(snippet.code))"
              >
                <PlayIcon :size="12" />
              </button>
            </div>
            <div v-if="snippet.output" class="output">
              <pre v-if="snippet.outputText !== null" :class="{ failed: snippet.output.error }">{{
                snippet.outputText
              }}</pre>
              <table v-else-if="snippet.output.set">
                <thead>
                  <tr>
                    <th v-for="column in snippet.output.set.columns" :key="column">{{ column }}</th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="(row, rowIndex) in snippet.output.set.rows" :key="rowIndex">
                    <td v-for="(cell, cellIndex) in row" :key="cellIndex">{{ cell ?? 'NULL' }}</td>
                  </tr>
                </tbody>
              </table>
            </div>
          </template>
        </li>
      </ol>
      <button class="primary" @click="reload">Try again</button>
      <p class="setup-detail">{{ session.queryStatsSetup }}</p>
    </div>
  </div>
  <DataGrid
    v-else
    ref="grid"
    v-model:selected="selected"
    :columns="columns"
    :rows="rows"
    :order-by="list.columnName"
    :descending="list.descending"
    title="Double-click a row to open it in the Query editor"
    @sort="sortColumn"
    @dblclick="openRow"
    @menu="openMenu"
  />
  <ContextMenu v-if="menu && picked" :x="menu.x" :y="menu.y" @close="closeMenu">
    <button role="menuitem" @click="grid?.copyRows()">
      Copy
      <span class="hint">{{ shortcuts.hint('C') }}</span>
    </button>
    <span class="menu-line" />
    <button role="menuitem" @click="askAi">Explain with AI</button>
    <button role="menuitem" @click="openPicked">Open in Query editor</button>
  </ContextMenu>

  <BottomBar :session="session" />
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

.filter {
  width: 180px;
  height: 20px;
  font-size: 11px;
}

label {
  display: flex;
  gap: 4px;
  align-items: center;
  font-size: 11px;
}

.days {
  width: 56px;
  height: 20px;
  font-size: 11px;
}
/* margin: auto on the panel centers it both ways. A tall panel still scrolls from its top. */
.setup {
  display: flex;
  flex: 1;
  overflow: auto;
  padding: 16px;
}

.setup-panel {
  width: 100%;
  max-width: 620px;
  margin: auto;
  font-size: 12px;
  /* The body turns selection off. The steps, snippets and server text can be selected. */
  -webkit-user-select: text;
  user-select: text;
}

.setup-panel button {
  -webkit-user-select: none;
  user-select: none;
}

.setup-title {
  margin: 0 0 8px;
  font-weight: 600;
}

ol {
  margin: 0 0 12px;
  padding-left: 18px;
}

li {
  margin-bottom: 8px;
  color: var(--text-2);
}

/* What a snippet gave when it ran. */
.output {
  margin-top: 4px;
  padding: 4px 8px;
  overflow-x: auto;
  border-left: 2px solid var(--border);
  color: var(--text);
  font-family: var(--font-mono);
  font-size: 11px;
}

.output .failed {
  color: var(--danger);
}

.output table {
  border-collapse: collapse;
}

.output th,
.output td {
  padding: 1px 8px 1px 0;
  text-align: left;
  white-space: nowrap;
}

.output th {
  color: var(--text-2);
  font-weight: 600;
}

/* Red, because the SQL changes the server. */
.run {
  color: var(--danger);
}

.option {
  display: block;
  margin-top: 6px;
}

.snippet {
  display: flex;
  gap: 6px;
  /* The text and the 22px buttons share one middle line. A taller snippet keeps even padding. */
  align-items: center;
  margin-top: 4px;
  padding: 4px 4px 4px 8px;
  border: 1px solid var(--border-soft);
  border-radius: 5px;
  background: var(--sidebar);
}

pre {
  flex: 1;
  margin: 0;
  overflow-x: auto;
  color: var(--text);
  font-family: var(--font-mono);
  font-size: 11px;
  line-height: 1.4;
  white-space: pre-wrap;
}

/* "Try again" sits on the right edge of the panel. */
.setup-panel > button.primary {
  display: flex;
  width: fit-content;
  margin-left: auto;
}

.setup-detail {
  margin: 12px 0 0;
  color: var(--text-3);
  font-size: 11px;
}
</style>
