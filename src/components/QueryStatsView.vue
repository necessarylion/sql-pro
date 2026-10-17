<script setup lang="ts">
import { computed, onMounted, reactive, ref, useTemplateRef, watch } from 'vue'
import { GaugeIcon, RefreshCwIcon, SparklesIcon } from 'lucide-vue-next'
import {
  QUERY_STATS_DAY_CHOICES,
  QUERY_STATS_MAX_DAYS,
  QUERY_STATS_MIN_DAYS,
} from '../constants/queryStats'
import { SHORTCUTS, injectRequired } from '../services/keys'
import { QueryStatList } from '../services/QueryStatList'
import { ExplainQueryPrompt } from '../services/ExplainQueryPrompt'
import type { Session } from '../services/Session'
import type { QueryStat, Row } from '../types'
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
</style>
