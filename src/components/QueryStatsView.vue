<script setup lang="ts">
import { computed, onMounted, reactive } from 'vue'
import { ChevronDownIcon, ChevronUpIcon, GaugeIcon, RefreshCwIcon } from 'lucide-vue-next'
import {
  QUERY_STATS_DAY_CHOICES,
  QUERY_STATS_MAX_DAYS,
  QUERY_STATS_MIN_DAYS,
} from '../constants/queryStats'
import { QueryStatList, type QueryStatColumn } from '../services/QueryStatList'
import type { Session } from '../services/Session'
import type { QueryStat } from '../types'
import BottomBar from './BottomBar.vue'

const props = defineProps<{ session: Session }>()
const list = reactive(new QueryStatList())
const shown = computed(showStats)
const count = computed(describeCount)
/** PostgreSQL keeps no last run time, so it cannot filter by days. */
const hasDays = computed(keepsLastRun)

const COLUMNS: { key: QueryStatColumn; label: string; number: boolean }[] = [
  { key: 'query', label: 'Query', number: false },
  { key: 'database', label: 'Database', number: false },
  { key: 'runs', label: 'Runs', number: true },
  { key: 'lastRun', label: 'Last run', number: false },
  { key: 'totalMs', label: 'Total', number: true },
  { key: 'averageMs', label: 'Average', number: true },
  { key: 'maxMs', label: 'Max', number: true },
  { key: 'rows', label: 'Rows', number: true },
]

// The stats change all the time. So load them each time the view opens.
onMounted(reload)

function reload(): void {
  props.session.run(props.session.loadQueryStats())
}

function showStats(): QueryStat[] {
  return list.visible(props.session.queryStats)
}

function keepsLastRun(): boolean {
  return props.session.config?.driver !== 'postgres'
}

function describeCount(): string {
  const all = props.session.queryStats.length
  const queries = `${all} ${all === 1 ? 'query' : 'queries'}`
  return shown.value.length === all ? queries : `${shown.value.length} of ${queries}`
}

/** What a cell shows. Times are in ms, so they get a unit. */
function cell(stat: QueryStat, key: QueryStatColumn): string {
  const value = stat[key]
  if (key === 'totalMs' || key === 'averageMs' || key === 'maxMs') {
    return QueryStatList.duration(value as number | null)
  }
  return value === null ? '' : String(value)
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
    <button class="icon" title="Refresh query stats" @click="reload">
      <RefreshCwIcon :size="13" />
    </button>
  </div>
  <div class="grid">
    <table>
      <thead>
        <tr>
          <th
            v-for="column in COLUMNS"
            :key="column.key"
            :class="{ number: column.number }"
            @click="list.sortBy(column.key)"
          >
            {{ column.label }}
            <template v-if="list.column === column.key">
              <ChevronDownIcon v-if="list.descending" :size="10" />
              <ChevronUpIcon v-else :size="10" />
            </template>
          </th>
        </tr>
      </thead>
      <tbody>
        <tr
          v-for="(stat, index) in shown"
          :key="index"
          title="Double-click to open in the Query editor"
          @dblclick="session.openInQuery(stat.query)"
        >
          <td
            v-for="column in COLUMNS"
            :key="column.key"
            :class="{ number: column.number, query: column.key === 'query' }"
            :title="column.key === 'query' ? stat.query : undefined"
          >
            {{ cell(stat, column.key) }}
          </td>
        </tr>
      </tbody>
    </table>
  </div>

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

/* The same look as the Processes view. */
.grid {
  flex: 1;
  min-height: 0;
  overflow: auto;
  background: var(--bg);
  font-size: 11px;
}

table {
  border-spacing: 0;
}

th,
td {
  height: 22px;
  padding: 0 8px;
  text-align: left;
  white-space: nowrap;
}

th {
  position: sticky;
  top: 0;
  z-index: 2;
  background: var(--header);
  border-right: 1px solid var(--border);
  border-bottom: 1px solid var(--border);
  color: var(--text-2);
  font-weight: 600;
  cursor: pointer;
}

td {
  border-right: 1px solid var(--border-soft);
  font-family: var(--font-mono);
  -webkit-user-select: text;
  user-select: text;
}

th.number,
td.number {
  text-align: right;
}

/* A long query shows in full on hover. */
td.query {
  max-width: 480px;
  overflow: hidden;
  text-overflow: ellipsis;
}

tbody tr {
  cursor: pointer;
}

tbody tr:last-child td {
  border-bottom: 1px solid var(--border-soft);
}

tbody tr:nth-child(even) {
  background: var(--row-alt);
}
</style>
