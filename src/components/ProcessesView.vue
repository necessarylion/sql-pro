<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { confirm } from '@tauri-apps/plugin-dialog'
import { ActivityIcon, CircleStopIcon, RefreshCwIcon, XIcon } from 'lucide-vue-next'
import { PROCESS_REFRESH_MS } from '../constants/processes'
import type { Session } from '../services/Session'
import type { DbProcess } from '../types'
import BottomBar from './BottomBar.vue'

const props = defineProps<{ session: Session }>()
const count = computed(describeCount)
const stopsQueries = computed(canStopQueries)
/** Auto refresh is off by default. */
const auto = ref(false)
let timer: ReturnType<typeof setInterval> | undefined

// Processes change all the time. So load the list each time the view opens.
onMounted(reload)
onUnmounted(stopTimer)

function reload(): void {
  props.session.run(props.session.loadProcesses())
}

/** Skips a turn while the tab is busy, so slow loads do not pile up. */
function tick(): void {
  if (!props.session.busy) {
    reload()
  }
}

function toggleAuto(): void {
  stopTimer()
  if (auto.value) {
    timer = setInterval(tick, PROCESS_REFRESH_MS)
  }
}

function stopTimer(): void {
  clearInterval(timer)
  timer = undefined
}

function describeCount(): string {
  const processes = props.session.processes.length
  return `${processes} ${processes === 1 ? 'process' : 'processes'}`
}

/** SQL Server can only end the whole session. */
function canStopQueries(): boolean {
  return props.session.config?.driver !== 'mssql'
}

/** Like "12s", "3m 5s" or "2h 4m". */
function duration(seconds: number | null): string {
  if (seconds === null) {
    return ''
  }
  if (seconds < 60) {
    return `${seconds}s`
  }
  if (seconds < 3600) {
    return `${Math.floor(seconds / 60)}m ${seconds % 60}s`
  }
  return `${Math.floor(seconds / 3600)}h ${Math.floor((seconds % 3600) / 60)}m`
}

/** Asks first. Stops the process only on yes. */
async function kill(process: DbProcess, queryOnly: boolean): Promise<void> {
  const owner = process.user ? ` of "${process.user}"` : ''
  const question = queryOnly
    ? `Stop the running query of process ${process.id}${owner}? The connection stays open.`
    : `Kill process ${process.id}${owner}? Its connection is closed and an open transaction is rolled back.`
  const sure = await confirm(question, {
    title: queryOnly ? 'Stop query' : 'Kill process',
    kind: 'warning',
  })
  if (sure) {
    await props.session.killProcess(process, queryOnly)
  }
}
</script>

<template>
  <div class="bar info">
    <ActivityIcon class="glyph" :size="13" />
    <span class="name">Processes</span>
    <span class="dim">{{ count }}</span>
    <span class="spacer" />
    <label class="dim" :title="`Load the list every ${PROCESS_REFRESH_MS / 1000} seconds`">
      <input v-model="auto" type="checkbox" @change="toggleAuto" />
      Auto refresh
    </label>
    <button class="icon" title="Refresh processes" @click="reload">
      <RefreshCwIcon :size="13" />
    </button>
  </div>
  <div class="grid">
    <table>
      <thead>
        <tr>
          <th>Id</th>
          <th>User</th>
          <th>Host</th>
          <th>Database</th>
          <th>State</th>
          <th>Time</th>
          <th>Query</th>
          <th />
        </tr>
      </thead>
      <tbody>
        <tr v-for="process in session.processes" :key="process.id">
          <td>
            {{ process.id }}
            <span v-if="process.current" class="pill">this tab</span>
          </td>
          <td>{{ process.user }}</td>
          <td>{{ process.host }}</td>
          <td>{{ process.database }}</td>
          <td>{{ process.state }}</td>
          <td class="number">{{ duration(process.seconds) }}</td>
          <td class="query" :title="process.query ?? ''">{{ process.query }}</td>
          <td>
            <button
              v-if="stopsQueries"
              class="icon"
              :title="process.current ? 'This is the connection of this tab' : 'Stop the query'"
              :disabled="process.current"
              @click="session.run(kill(process, true))"
            >
              <CircleStopIcon :size="12" />
            </button>
            <button
              class="icon"
              :title="process.current ? 'This is the connection of this tab' : 'Kill the process'"
              :disabled="process.current"
              @click="session.run(kill(process, false))"
            >
              <XIcon :size="12" />
            </button>
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

label {
  display: flex;
  gap: 4px;
  align-items: center;
  font-size: 11px;
}

/* The same look as the Users view. */
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
}

td {
  border-right: 1px solid var(--border-soft);
  font-family: var(--font-mono);
  -webkit-user-select: text;
  user-select: text;
}

td.number {
  text-align: right;
}

/* A long query shows in full on hover. */
td.query {
  max-width: 420px;
  overflow: hidden;
  text-overflow: ellipsis;
}

tbody tr:last-child td {
  border-bottom: 1px solid var(--border-soft);
}

tbody tr:nth-child(even) {
  background: var(--row-alt);
}

td .pill {
  height: 14px;
  margin-left: 4px;
  padding: 0 6px;
  font-family: var(--font-ui);
  font-size: 10px;
}

td button.icon {
  height: 18px;
}
</style>
