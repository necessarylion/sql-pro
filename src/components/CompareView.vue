<script setup lang="ts">
import { computed, onMounted } from 'vue'
import { ArrowRightIcon, GitCompareArrowsIcon, PlayIcon, TerminalIcon } from 'lucide-vue-next'
import { CHANGE_LABELS, DATA_LOSS_ACTIONS } from '../constants/schemaSync'
import { SESSION_MANAGER, injectRequired } from '../services/keys'
import { SchemaSync, type ChangeGroup, type TableChanges } from '../services/SchemaSync'
import type { Session } from '../services/Session'
import type { SchemaChange } from '../types'
import BottomBar from './BottomBar.vue'

const props = defineProps<{ session: Session }>()
const manager = injectRequired(SESSION_MANAGER)
const sync = computed(readSync)
const tabs = computed(connectedTabs)
const source = computed(readSource)
const target = computed(readTarget)
const problem = computed(describeProblem)
const groups = computed(readGroups)
const summary = computed(describeSummary)

onMounted(pickDefaults)

function readSync(): SchemaSync {
  return props.session.schemaSync
}

/** The tabs that have a connection. Only those can be compared. */
function connectedTabs(): Session[] {
  return manager.sessions.filter(isConnected)
}

function isConnected(tab: Session): boolean {
  return tab.config !== null
}

function findTab(id: string): Session | undefined {
  for (const tab of manager.sessions) {
    if (tab.id === id) {
      return tab
    }
  }
  return undefined
}

function readSource(): Session | undefined {
  return findTab(sync.value.sourceId)
}

function readTarget(): Session | undefined {
  return findTab(sync.value.targetId)
}

function describeProblem(): string {
  return SchemaSync.problem(source.value, target.value)
}

function readGroups(): ChangeGroup[] {
  return sync.value.groups()
}

/** The source is this tab. The target is the first other connected tab. */
function pickDefaults(): void {
  if (!findTab(sync.value.sourceId)) {
    sync.value.sourceId = props.session.id
  }
  if (!findTab(sync.value.targetId)) {
    const other = tabs.value.find(isOtherTab)
    sync.value.targetId = other?.id ?? ''
  }
}

function isOtherTab(tab: Session): boolean {
  return tab.id !== sync.value.sourceId
}

/** Like "Shop dev · shop". SQLite shows no database: it is the long file path. */
function tabLabel(tab: Session): string {
  const config = tab.config
  if (!config || config.driver === 'sqlite' || !config.database) {
    return tab.title
  }
  return `${tab.title} · ${config.database}`
}

function describeSummary(): string {
  const changes = sync.value.changes
  if (changes === null) {
    return ''
  }
  const picked = sync.value.picked.length
  const all = changes.length
  return `${all} ${all === 1 ? 'difference' : 'differences'} · ${picked} picked`
}

function compare(): void {
  props.session.run(sync.value.compare(source.value, target.value))
}

function makeSql(): void {
  props.session.run(sync.value.makeSql())
}

/** Puts the SQL into the Query view of the target tab and shows that tab. */
function openInQuery(): void {
  if (target.value) {
    sync.value.openInQuery(target.value)
    manager.activate(target.value)
  }
}

function runOnTarget(): void {
  if (source.value && target.value) {
    props.session.run(runAndReport(source.value, target.value))
  }
}

async function runAndReport(from: Session, to: Session): Promise<void> {
  if (await sync.value.runOnTarget(from, to)) {
    props.session.status = `The SQL ran on "${to.title}". Compared again.`
  }
}

function actionLabel(change: SchemaChange): string {
  return CHANGE_LABELS[change.action]
}

function losesData(change: SchemaChange): boolean {
  return DATA_LOSS_ACTIONS.includes(change.action)
}

/** True for a new or a dropped table. It is one difference. A changed table has many. */
function isTableLevel(table: TableChanges): boolean {
  const first = table.changes[0]
  return first?.action === 'createTable' || first?.action === 'dropTable'
}
</script>

<template>
  <div class="bar info">
    <GitCompareArrowsIcon class="glyph" :size="13" />
    <span class="name">Compare</span>
    <select v-model="sync.sourceId" title="Source: the database to copy the schema from">
      <option value="" disabled>Source…</option>
      <option v-for="tab in tabs" :key="tab.id" :value="tab.id">{{ tabLabel(tab) }}</option>
    </select>
    <ArrowRightIcon class="dim" :size="12" />
    <select v-model="sync.targetId" title="Target: the database to change">
      <option value="" disabled>Target…</option>
      <option v-for="tab in tabs" :key="tab.id" :value="tab.id">{{ tabLabel(tab) }}</option>
    </select>
    <button :disabled="problem !== '' || session.busy" @click="compare">Compare</button>
    <span v-if="problem" class="dim problem" :title="problem">{{ problem }}</span>
    <span class="spacer" />
    <span class="dim">{{ summary }}</span>
  </div>

  <div class="compare">
    <div class="changes">
      <p v-if="sync.changes === null" class="empty dim">
        Pick a source and a target tab, then press Compare. The target is changed to match the
        source. Tables, columns, indexes and foreign keys are compared.
      </p>
      <p v-else-if="sync.changes.length === 0" class="empty dim">
        No differences. The target has the same schema as the source.
      </p>
      <template v-for="group in groups" :key="group.title">
        <div class="section">{{ group.title }}</div>
        <template v-for="table in group.tables" :key="table.label">
          <label v-if="isTableLevel(table)" class="change">
            <input
              type="checkbox"
              :checked="sync.isPicked(table.changes[0])"
              @change="sync.toggle(table.changes[0])"
            />
            <span class="table-name">{{ table.label }}</span>
            <span class="detail dim">{{ table.changes[0].detail }}</span>
          </label>
          <template v-else>
            <label class="change">
              <input
                type="checkbox"
                :checked="sync.allPicked(table.changes)"
                @change="sync.toggleAll(table.changes)"
              />
              <span class="table-name">{{ table.label }}</span>
            </label>
            <label v-for="change in table.changes" :key="change.id" class="change inner">
              <input
                type="checkbox"
                :checked="sync.isPicked(change)"
                @change="sync.toggle(change)"
              />
              <span class="action" :class="{ loss: losesData(change) }">{{
                actionLabel(change)
              }}</span>
              <span class="item">{{ change.name }}</span>
              <span class="detail dim" :title="change.detail">{{ change.detail }}</span>
            </label>
          </template>
        </template>
      </template>
    </div>

    <div class="sql">
      <div class="bar">
        <span class="name">SQL for the target</span>
        <span class="spacer" />
        <button
          v-if="sync.changes?.length && !sync.sql"
          :disabled="sync.picked.length === 0 || session.busy"
          @click="makeSql"
        >
          Make SQL
        </button>
        <button
          :disabled="!sync.script || session.busy"
          title="Put the SQL into the Query view of the target tab. Nothing runs."
          @click="openInQuery"
        >
          <TerminalIcon :size="12" />
          Open in Query view
        </button>
        <button
          class="run"
          :disabled="!sync.script || session.busy"
          title="Run the SQL on the target. It asks first."
          @click="runOnTarget"
        >
          <PlayIcon :size="12" />
          Run on target
        </button>
      </div>
      <ul v-if="sync.sql?.warnings.length" class="warnings">
        <li v-for="warning in sync.sql.warnings" :key="warning">{{ warning }}</li>
      </ul>
      <pre v-if="sync.script" class="script">{{ sync.script }}</pre>
      <p v-else-if="sync.changes?.length && !sync.sql" class="empty dim">
        The picks changed. Press Make SQL to see the new SQL.
      </p>
      <p v-else-if="sync.sql" class="empty dim">No SQL for the picked differences.</p>
    </div>
  </div>

  <BottomBar :session="session" />
</template>

<style scoped>
.info {
  gap: 8px;
  padding: 0 10px;
}

.glyph {
  color: var(--accent-text);
}

.name {
  font-size: calc(12px * var(--font-scale));
  font-weight: 600;
}

.info select {
  max-width: 220px;
  height: 22px;
  padding: 0 6px;
  border-radius: 5px;
}

.problem {
  overflow: hidden;
  text-overflow: ellipsis;
}

.compare {
  display: flex;
  flex: 1;
  min-height: 0;
}

.changes {
  flex: 1;
  min-width: 0;
  overflow: auto;
  padding: 4px 0;
  border-right: 1px solid var(--border);
}

.sql {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-width: 0;
}

.empty {
  margin: 0;
  padding: 12px;
  white-space: normal;
}

.section {
  padding: 8px 10px 3px;
  color: var(--text-3);
  font-size: calc(10px * var(--font-scale));
  font-weight: 600;
  text-transform: uppercase;
}

.change {
  display: flex;
  gap: 6px;
  align-items: center;
  height: 22px;
  padding: 0 10px;
  font-size: calc(11px * var(--font-scale));
  white-space: nowrap;
}

.change:hover {
  background: var(--row-alt);
}

.change.inner {
  padding-left: 28px;
}

.table-name {
  font-weight: 600;
}

.action {
  color: var(--text-2);
}

.action.loss {
  color: var(--danger);
}

.item {
  font-family: var(--font-mono);
}

.detail {
  overflow: hidden;
  font-family: var(--font-mono);
  text-overflow: ellipsis;
}

/* Run changes the target database, so it has the warning colour. */
button.run:not(:disabled) {
  color: var(--warn);
}

.warnings {
  margin: 0;
  padding: 6px 10px 6px 26px;
  border-bottom: 1px solid var(--border);
  color: var(--warn);
  font-size: calc(11px * var(--font-scale));
  -webkit-user-select: text;
  user-select: text;
}

.script {
  flex: 1;
  margin: 0;
  padding: 8px 10px;
  overflow: auto;
  font-family: var(--font-mono);
  font-size: calc(11px * var(--font-scale));
  -webkit-user-select: text;
  user-select: text;
}
</style>
