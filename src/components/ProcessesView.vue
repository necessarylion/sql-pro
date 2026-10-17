<script setup lang="ts">
import { computed, onMounted, onUnmounted, reactive, ref, watch } from 'vue'
import { confirm } from '@tauri-apps/plugin-dialog'
import { ActivityIcon, CircleStopIcon, RefreshCwIcon, XIcon } from 'lucide-vue-next'
import { PROCESS_REFRESH_MS } from '../constants/processes'
import { ProcessList } from '../services/ProcessList'
import type { Session } from '../services/Session'
import type { DbProcess, Row } from '../types'
import BottomBar from './BottomBar.vue'
import DataGrid from './DataGrid.vue'

const props = defineProps<{ session: Session }>()
const list = reactive(new ProcessList())
const columns = ProcessList.columns()
const shown = computed(showProcesses)
const rows = computed(gridRows)
const selected = ref<number[]>([])
const target = computed(selectedProcess)
const stopTitle = computed(describeStop)
const count = computed(describeCount)
const stopsQueries = computed(canStopQueries)
/** Auto refresh is off by default. */
const auto = ref(false)
let timer: ReturnType<typeof setInterval> | undefined

// Processes change all the time. So load the list each time the view opens.
onMounted(reload)
onUnmounted(stopTimer)
// A new sort or load moves the rows. The old selection would point to other rows.
watch(shown, clearSelection)

function sortColumn(name: string): void {
  list.sortByName(name)
}

function reload(): void {
  props.session.run(props.session.loadProcesses())
}

function clearSelection(): void {
  selected.value = []
}

function showProcesses(): DbProcess[] {
  return list.visible(props.session.processes)
}

function gridRows(): Row[] {
  const rows: Row[] = []
  for (const process of shown.value) {
    rows.push(ProcessList.row(process))
  }
  return rows
}

/** The last selected process. The connection of this tab cannot be stopped, so it gives none. */
function selectedProcess(): DbProcess | null {
  const index = selected.value.at(-1)
  const process = index === undefined ? undefined : shown.value[index]
  return process && !process.current ? process : null
}

function describeStop(): string {
  const index = selected.value.at(-1)
  if (index === undefined) {
    return 'Select a process first'
  }
  return target.value ? '' : 'This is the connection of this tab'
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

function stop(queryOnly: boolean): void {
  if (target.value) {
    props.session.run(kill(target.value, queryOnly))
  }
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
    <button
      v-if="stopsQueries"
      class="icon warn"
      :title="stopTitle || 'Stop the query of the selected process'"
      :disabled="!target"
      @click="stop(true)"
    >
      <CircleStopIcon :size="13" />
    </button>
    <button
      class="icon danger"
      :title="stopTitle || 'Kill the selected process'"
      :disabled="!target"
      @click="stop(false)"
    >
      <XIcon :size="13" />
    </button>
    <label class="dim" :title="`Load the list every ${PROCESS_REFRESH_MS / 1000} seconds`">
      <input v-model="auto" type="checkbox" @change="toggleAuto" />
      Auto refresh
    </label>
    <button class="icon" title="Refresh processes" @click="reload">
      <RefreshCwIcon :size="13" />
    </button>
  </div>
  <DataGrid
    v-model:selected="selected"
    :columns="columns"
    :rows="rows"
    :order-by="list.columnName"
    :descending="list.descending"
    @sort="sortColumn"
  />

  <BottomBar :session="session" />
</template>

<style scoped>
/* Stop query is a warning, kill is a delete. Only the icon has the colour. */
button.warn {
  color: var(--warn);
}

/* The colours above and the global danger colour win over the disabled grey. Keep the normal disabled look. */
button.warn:disabled,
button.danger:disabled {
  color: var(--text-3);
}

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
</style>
