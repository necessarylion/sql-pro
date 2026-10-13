<script setup lang="ts">
import { computed, nextTick, ref, type Component } from 'vue'
import { confirm, save } from '@tauri-apps/plugin-dialog'
import {
  ChevronsUpDownIcon,
  DatabaseIcon,
  DownloadIcon,
  EyeIcon,
  RefreshCwIcon,
  SearchIcon,
  TableIcon,
  TablePropertiesIcon,
  TerminalIcon,
  UnplugIcon,
} from 'lucide-vue-next'
import { VIEW_LABELS } from '../constants'
import type { Session } from '../services/Session'
import type { SessionView, TableRef } from '../types'
import ContentView from './ContentView.vue'
import QueryView from './QueryView.vue'
import StructureView from './StructureView.vue'

const props = defineProps<{ session: Session }>()

const VIEW_ICONS: Record<SessionView, Component> = {
  structure: TablePropertiesIcon,
  content: TableIcon,
  query: TerminalIcon,
}

// The size of the table menu. It keeps the menu inside the window.
const MENU_WIDTH = 170
const MENU_HEIGHT = 56

const search = ref('')
/** The table menu that is open now, and where it is on screen. */
const menu = ref<{ table: TableRef; x: number; y: number } | null>(null)
const menuElement = ref<HTMLElement | null>(null)
const filteredTables = computed(matchingTables)
const groups = computed(tableGroups)
const connection = computed(describeConnection)
const summary = computed(describeTables)
const backupBlocked = computed(hasNoDatabase)

/** Shows the schema only when it is not the default one. */
function label(table: TableRef): string {
  return table.schema && table.schema !== 'public' ? `${table.schema}.${table.name}` : table.name
}

function matchingTables(): TableRef[] {
  const needle = search.value.trim().toLowerCase()
  const matches: TableRef[] = []
  for (const table of props.session.tables) {
    if (label(table).toLowerCase().includes(needle)) {
      matches.push(table)
    }
  }
  return matches
}

/** The matching tables, then the matching views. Each group has its own title. */
function tableGroups(): { title: string; icon: Component; items: TableRef[] }[] {
  const tables: TableRef[] = []
  const views: TableRef[] = []
  for (const table of filteredTables.value) {
    if (table.kind === 'view') {
      views.push(table)
    } else {
      tables.push(table)
    }
  }
  return [
    { title: 'Tables', icon: TableIcon, items: tables },
    { title: 'Views', icon: EyeIcon, items: views },
  ]
}

/** `user@host:port`, or the file name for SQLite. */
function describeConnection(): string {
  const config = props.session.config
  if (!config) {
    return ''
  }
  if (config.driver === 'sqlite') {
    return config.database.split(/[\\/]/).pop() ?? ''
  }
  return `${config.user}@${config.host}:${config.port}`
}

/** Like "13 tables · 2 views". Counts all of them, not only the matching ones. */
function describeTables(): string {
  let views = 0
  for (const table of props.session.tables) {
    if (table.kind === 'view') {
      views += 1
    }
  }
  const tables = props.session.tables.length - views
  return `${tables} ${tables === 1 ? 'table' : 'tables'} · ${views} ${views === 1 ? 'view' : 'views'}`
}

/** Opens the table menu at the mouse. It takes the focus, so Escape closes it. */
async function openMenu(event: MouseEvent, table: TableRef): Promise<void> {
  menu.value = {
    table,
    x: Math.min(event.clientX, window.innerWidth - MENU_WIDTH - 4),
    y: Math.min(event.clientY, window.innerHeight - MENU_HEIGHT - 4),
  }
  await nextTick()
  menuElement.value?.focus()
}

function closeMenu(): void {
  menu.value = null
}

/** Asks first. Truncates or drops the table of the menu only on yes. */
async function choose(action: 'Truncate' | 'Drop'): Promise<void> {
  const table = menu.value?.table
  closeMenu()
  if (!table) {
    return
  }
  const session = props.session
  const kind = table.kind
  let effect = `The ${kind} and all its data are deleted.`
  if (action === 'Truncate') {
    effect = 'All rows in it are deleted.'
  } else if (kind === 'view') {
    effect = 'The view is deleted. Its tables keep their data.'
  }
  const sure = await confirm(
    `${action} ${kind} "${label(table)}"? ${effect} This cannot be undone.`,
    { title: `${action} ${kind}`, kind: 'warning' },
  )
  if (sure) {
    await session.run(action === 'Drop' ? session.dropTable(table) : session.truncateTable(table))
  }
}

/** MySQL can be connected without a database. Then there is nothing to back up. */
function hasNoDatabase(): boolean {
  return props.session.config?.driver === 'mysql' && props.session.config.database === ''
}

/** Asks where to save, asks to confirm, then writes the SQL dump. Does nothing on cancel or no. */
async function backup(): Promise<void> {
  const session = props.session
  const path = await save({
    title: 'Back up database',
    defaultPath: session.backupName(new Date()),
    filters: [{ name: 'SQL', extensions: ['sql'] }],
  })
  if (!path) {
    return
  }
  // SQLite has no database name, so show the file name.
  const config = session.config
  const name = config?.driver === 'sqlite' ? connection.value : (config?.database ?? '')
  const sure = await confirm(
    `Back up database "${name}" to ${path}? A large database can take a few minutes.`,
    { title: 'Back up database', kind: 'info' },
  )
  if (sure) {
    await session.run(session.backup(path))
  }
}

function changeDatabase(event: Event): void {
  const name = (event.target as HTMLSelectElement).value
  props.session.run(props.session.useDatabase(name))
}
</script>

<template>
  <div class="workspace">
    <div class="bar toolbar">
      <div v-if="session.config?.driver !== 'sqlite'" class="database">
        <DatabaseIcon :size="12" />
        <select :value="session.config?.database" title="Database" @change="changeDatabase">
          <option value="" disabled>Choose database…</option>
          <option v-for="name in session.databases" :key="name">{{ name }}</option>
        </select>
        <ChevronsUpDownIcon :size="11" />
      </div>
      <div class="segmented">
        <button
          v-for="(text, view) in VIEW_LABELS"
          :key="view"
          :class="{ active: session.view === view }"
          @click="session.view = view"
        >
          <component :is="VIEW_ICONS[view]" :size="12" />
          {{ text }}
        </button>
      </div>
      <span class="spacer" />
      <span class="status-dot" />
      <span class="connection">{{ connection }}</span>
      <button class="icon" title="Refresh tables" @click="session.run(session.loadTables())">
        <RefreshCwIcon :size="13" />
      </button>
      <button
        class="icon"
        :title="backupBlocked ? 'Choose a database first' : 'Back up this database to a SQL file'"
        :disabled="backupBlocked"
        @click="backup"
      >
        <DownloadIcon :size="13" />
      </button>
      <button class="icon" title="Disconnect" @click="session.run(session.disconnect())">
        <UnplugIcon :size="13" />
      </button>
    </div>

    <div class="body">
      <aside class="sidebar">
        <div class="search">
          <SearchIcon :size="11" />
          <input v-model="search" placeholder="Filter tables" spellcheck="false" />
        </div>
        <div class="tables">
          <template v-for="group in groups" :key="group.title">
            <div class="section">
              <span>{{ group.title }}</span>
              <span class="count">{{ group.items.length }}</span>
            </div>
            <ul class="list">
              <li
                v-for="table in group.items"
                :key="label(table)"
                :class="{ selected: session.isOpen(table) }"
                :title="label(table)"
                @click="session.run(session.selectTable(table))"
                @contextmenu.prevent="openMenu($event, table)"
              >
                <component :is="group.icon" :size="12" />
                <span class="name">{{ label(table) }}</span>
              </li>
            </ul>
          </template>
        </div>
        <button
          class="sidebar-footer"
          title="Refresh tables"
          @click="session.run(session.loadTables())"
        >
          <RefreshCwIcon :size="12" />
          {{ summary }}
        </button>
      </aside>

      <main class="main">
        <!-- Only hidden, never removed. So the editor keeps its undo history and height. -->
        <div v-show="session.view === 'query'" class="query">
          <QueryView :session="session" />
        </div>
        <template v-if="session.view !== 'query'">
          <!-- No table is open, so no view with a bottom bar. Show the error or the status here. -->
          <div v-if="!session.page" class="empty">
            <span v-if="session.error" class="error failure">{{ session.error }}</span>
            <template v-else-if="session.busy">Working…</template>
            <template v-else-if="session.status">{{ session.status }}</template>
            <template v-else-if="!session.table">Select a table</template>
          </div>
          <ContentView v-else-if="session.view === 'content'" :session="session" />
          <StructureView v-else :session="session" />
        </template>
      </main>
    </div>

    <!-- The backdrop covers the window. A click, a right-click or a scroll on it closes the menu. -->
    <div
      v-if="menu"
      class="menu-backdrop"
      @click="closeMenu"
      @contextmenu.prevent="closeMenu"
      @wheel="closeMenu"
    >
      <div
        ref="menuElement"
        class="menu"
        role="menu"
        tabindex="-1"
        :style="{ left: `${menu.x}px`, top: `${menu.y}px` }"
        @keydown.esc="closeMenu"
      >
        <button v-if="menu.table.kind !== 'view'" role="menuitem" @click.stop="choose('Truncate')">
          Truncate table…
        </button>
        <button role="menuitem" @click.stop="choose('Drop')">Drop {{ menu.table.kind }}…</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.workspace,
.main,
.query,
.sidebar {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
}

.workspace,
.main,
.query {
  flex: 1;
}

.toolbar {
  gap: 10px;
  height: 40px;
  padding: 0 10px;
}

/* A real select. The two icons lie over it and let clicks through. */
.database {
  position: relative;
  display: flex;
  flex: none;
  align-items: center;
  justify-content: space-between;
  width: 170px;
  height: 24px;
  padding: 0 7px;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 5px;
  color: var(--text-2);
}

.database svg {
  z-index: 1;
  pointer-events: none;
}

.database select {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  padding: 0 22px 0 25px;
  border: 0;
  border-radius: 5px;
  background: none;
  color: var(--text);
  font-size: 12px;
  font-weight: 500;
  text-overflow: ellipsis;
  appearance: none;
}

.segmented {
  height: 24px;
  border-radius: 6px;
}

.toolbar .segmented button {
  gap: 5px;
  height: 20px;
  border-radius: 4px;
}

.segmented button.active svg {
  color: var(--accent-text);
}

.status-dot {
  flex: none;
  width: 7px;
  height: 7px;
  margin-right: -4px;
  background: var(--success);
  border-radius: 50%;
}

.connection {
  overflow: hidden;
  color: var(--text-2);
  font-family: var(--font-mono);
  text-overflow: ellipsis;
}

.body {
  display: flex;
  flex: 1;
  min-height: 0;
}

.sidebar {
  flex: none;
  width: 210px;
  background: var(--sidebar);
  border-right: 1px solid var(--border);
}

.tables {
  flex: 1;
  overflow: auto;
}

.menu-backdrop {
  position: fixed;
  inset: 0;
  z-index: 100;
}

/* Looks like a native macOS menu. */
.menu {
  position: fixed;
  display: flex;
  flex-direction: column;
  min-width: 170px;
  padding: 5px;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 6px;
  outline: none;
  box-shadow: 0 6px 20px #00000040;
}

.menu button {
  justify-content: flex-start;
  height: 22px;
  padding: 0 8px;
  border: 0;
  border-radius: 4px;
  background: none;
  color: var(--danger);
  font-size: 12px;
  font-weight: 400;
}

.menu button:hover,
.menu button:focus-visible {
  background: var(--accent);
  color: var(--on-accent);
  outline: none;
}

.failure {
  max-width: 80%;
  max-height: 80%;
  overflow-y: auto;
  text-align: center;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}
</style>
