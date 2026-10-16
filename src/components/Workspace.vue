<script setup lang="ts">
import {
  computed,
  nextTick,
  onMounted,
  onUnmounted,
  reactive,
  ref,
  useTemplateRef,
  watch,
  type Component,
} from 'vue'
import { confirm, open, save } from '@tauri-apps/plugin-dialog'
import {
  ChevronRightIcon,
  ChevronsUpDownIcon,
  DatabaseIcon,
  DownloadIcon,
  EyeIcon,
  LoaderCircleIcon,
  PlusIcon,
  RefreshCwIcon,
  SearchIcon,
  SquareFunctionIcon,
  TableIcon,
  TablePropertiesIcon,
  TerminalIcon,
  UnplugIcon,
  UploadIcon,
  UsersIcon,
  WorkflowIcon,
  ZapIcon,
} from 'lucide-vue-next'
import { OBJECT_KIND_LABELS, VIEW_LABELS } from '../constants'
import { SESSION_MANAGER, SHORTCUTS, injectRequired } from '../services/keys'
import { Session } from '../services/Session'
import { TableSelection } from '../services/TableSelection'
import type { DbObject, DbObjectKind, SessionView, TableRef } from '../types'
import ContentView from './ContentView.vue'
import ContextMenu from './ContextMenu.vue'
import DefinitionView from './DefinitionView.vue'
import DiagramView from './DiagramView.vue'
import EnvBadge from './EnvBadge.vue'
import ExportDialog from './ExportDialog.vue'
import ImportDialog from './ImportDialog.vue'
import PaneResizer from './PaneResizer.vue'
import QueryView from './QueryView.vue'
import StructureView from './StructureView.vue'
import TableDialog from './TableDialog.vue'
import UsersView from './UsersView.vue'

const props = defineProps<{ session: Session }>()
const manager = injectRequired(SESSION_MANAGER)
const shortcuts = injectRequired(SHORTCUTS)

const VIEW_ICONS: Record<SessionView, Component> = {
  structure: TablePropertiesIcon,
  content: TableIcon,
  query: TerminalIcon,
  users: UsersIcon,
  diagram: WorkflowIcon,
}

// The size of the table menu. It keeps the menu inside the window.
const MENU_WIDTH = 170
const MENU_HEIGHT = 165
// The width of the Export and Import submenus. It opens to the left when there is no room on the right.
const SUBMENU_WIDTH = 290
// The most table names a drop or truncate confirm lists. The rest are counted.
const CONFIRM_NAMES = 10

const search = ref('')
/**
 * The table menu that is open now, and where it is on screen. No table: the menu of the empty space.
 * `tables` are the tables it acts on: the selection, or only the clicked table.
 * `flip` opens the submenus to the left.
 */
const menu = ref<{
  table: TableRef | null
  tables: TableRef[]
  x: number
  y: number
  flip: boolean
} | null>(null)
/** The tables that are selected in the sidebar with Shift-click and Cmd-click (Ctrl on Windows and Linux). */
const selection = reactive(new TableSelection())
/** The submenu that is shown now, if any. */
const submenu = ref<'export' | 'import' | null>(null)
const tableDialog = useTemplateRef<InstanceType<typeof TableDialog>>('tableDialog')
const exportDialog = useTemplateRef<InstanceType<typeof ExportDialog>>('exportDialog')
const importDialog = useTemplateRef<InstanceType<typeof ImportDialog>>('importDialog')
const menuElement = ref<HTMLElement | null>(null)
const filteredTables = computed(matchingTables)
const groups = computed(tableGroups)
const connection = computed(describeConnection)
const summary = computed(describeTables)
const backupBlocked = computed(hasNoDatabase)
const exportAsked = computed(readExportAsked)
const database = computed(readDatabase)
const triggers = computed(matchingTriggers)
const routines = computed(matchingRoutines)
/** The trigger and routine groups that are closed. */
const collapsed = reactive({ triggers: false, routines: false })
/** The menu of a trigger or routine that is open now. */
const objectMenu = ref<{ object: DbObject; x: number; y: number } | null>(null)

watch(exportAsked, exportFromShortcut)
watch(database, clearSelection)
onMounted(listenForEscape)
onUnmounted(stopEscape)

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

/** The objects of these kinds whose name matches the filter. */
function matchingObjects(kinds: DbObjectKind[]): DbObject[] {
  const needle = search.value.trim().toLowerCase()
  const matches: DbObject[] = []
  for (const object of props.session.objects) {
    if (kinds.includes(object.kind) && objectLabel(object).toLowerCase().includes(needle)) {
      matches.push(object)
    }
  }
  return matches
}

function matchingTriggers(): DbObject[] {
  return matchingObjects(['trigger'])
}

function matchingRoutines(): DbObject[] {
  return matchingObjects(['procedure', 'function'])
}

/** The name, with the schema when it is not the default one, and the argument types of a PostgreSQL routine. */
function objectLabel(object: DbObject): string {
  const name =
    object.schema && object.schema !== 'public' ? `${object.schema}.${object.name}` : object.name
  return object.args === null ? name : `${name}(${object.args})`
}

/** Where the object is, like "on orders" for a trigger. */
function objectTitle(object: DbObject): string {
  const table = object.table ? ` on ${object.table}` : ''
  return `${OBJECT_KIND_LABELS[object.kind]} ${objectLabel(object)}${table}`
}

function toggle(group: 'triggers' | 'routines'): void {
  collapsed[group] = !collapsed[group]
}

function showDefinition(object: DbObject): void {
  closeMenu()
  closeObjectMenu()
  selection.clear()
  props.session.run(props.session.showDefinition(object))
}

function openObjectMenu(event: MouseEvent, object: DbObject): void {
  objectMenu.value = { object, x: event.clientX, y: event.clientY }
}

function closeObjectMenu(): void {
  objectMenu.value = null
}

/** Opens the SQL of the object in the Query view. */
async function editObject(object: DbObject): Promise<void> {
  await props.session.showDefinition(object)
  props.session.editDefinition()
}

/** Asks first. Drops the trigger or routine only on yes. */
async function dropObject(object: DbObject): Promise<void> {
  closeObjectMenu()
  const kind = OBJECT_KIND_LABELS[object.kind]
  const effect =
    object.kind === 'trigger' ? 'Its table and data stay.' : 'Code that calls it stops working.'
  const sure = await confirm(
    `Drop ${kind.toLowerCase()} "${objectLabel(object)}"? ${effect} This cannot be undone.`,
    { title: `Drop ${kind.toLowerCase()}`, kind: 'warning' },
  )
  if (sure) {
    await props.session.run(props.session.dropObject(object))
  }
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

/**
 * Opens the table menu at the mouse. It takes the focus, so Escape closes it.
 * Pass null for the menu of the empty space. It is not shown when there is no database.
 */
async function openMenu(event: MouseEvent, table: TableRef | null): Promise<void> {
  if (!table && backupBlocked.value) {
    return
  }
  // A table outside the selection: the menu is for that table only.
  if (table && !selection.has(label(table))) {
    selection.only(label(table))
  }
  const x = Math.min(event.clientX, window.innerWidth - MENU_WIDTH - 4)
  menu.value = {
    table,
    tables: table ? selectedTables() : [],
    x,
    y: Math.min(event.clientY, window.innerHeight - MENU_HEIGHT - 4),
    flip: x + MENU_WIDTH + SUBMENU_WIDTH > window.innerWidth,
  }
  submenu.value = null
  await nextTick()
  menuElement.value?.focus()
}

function closeMenu(): void {
  menu.value = null
}

function showSubmenu(name: 'export' | 'import'): void {
  submenu.value = name
}

function hideSubmenu(): void {
  submenu.value = null
}

function readExportAsked(): number {
  return props.session.exportAsked
}

function readDatabase(): string {
  return props.session.config?.database ?? ''
}

/** The shown tables that are selected, in the order of the list. */
function selectedTables(): TableRef[] {
  const tables: TableRef[] = []
  for (const group of groups.value) {
    for (const table of group.items) {
      if (selection.has(label(table))) {
        tables.push(table)
      }
    }
  }
  return tables
}

/** The names of the shown tables, in the order of the list. Shift-click selects a range of it. */
function shownNames(): string[] {
  const names: string[] = []
  for (const group of groups.value) {
    for (const table of group.items) {
      names.push(label(table))
    }
  }
  return names
}

function clearSelection(): void {
  selection.clear()
}

/** A click on the empty space of the list, not on a table, clears the selection. */
function clickList(event: MouseEvent): void {
  if (!(event.target as HTMLElement).closest('li')) {
    selection.clear()
  }
}

// Capture runs before the menu sees the key. So an Escape that closes the menu keeps the selection.
function listenForEscape(): void {
  window.addEventListener('keydown', escapeSelection, true)
}

function stopEscape(): void {
  window.removeEventListener('keydown', escapeSelection, true)
}

/** Escape clears the selection, but not while the table menu is open. */
function escapeSelection(event: KeyboardEvent): void {
  if (event.key === 'Escape' && !menu.value && !event.defaultPrevented) {
    selection.clear()
  }
}

/** The export shortcut: the selected tables are checked at the start, or else the open table. */
function exportFromShortcut(): void {
  const selected = selectedTables()
  const table = props.session.table
  exportDialog.value?.openTables(selected.length > 0 ? selected : table ? [table] : [])
}

/** Opens the export of many tables, with the tables of the menu checked. */
function exportTables(tables: TableRef[]): void {
  closeMenu()
  exportDialog.value?.openTables(tables)
}

/** Opens the export of one table, where the columns can be chosen. */
function exportColumns(table: TableRef): void {
  closeMenu()
  void exportDialog.value?.openColumns(table)
}

/** Asks for a CSV file, then opens the import dialog for it. Does nothing on cancel. */
async function importCsv(table: TableRef): Promise<void> {
  closeMenu()
  const path = await open({
    title: `Import CSV into "${label(table)}"`,
    filters: [{ name: 'CSV', extensions: ['csv', 'txt'] }],
  })
  if (path) {
    await importDialog.value?.openFile(table, path)
  }
}

/** Asks for a SQL file, asks to confirm, then runs all its statements. */
async function importSql(): Promise<void> {
  closeMenu()
  const session = props.session
  const path = await open({
    title: 'Import SQL file',
    filters: [{ name: 'SQL', extensions: ['sql'] }],
  })
  if (!path) {
    return
  }
  const sure = await confirm(
    `Run all statements in ${path} on database "${databaseLabel()}"? This can change or delete data.`,
    { title: 'Import SQL file', kind: 'warning' },
  )
  if (sure) {
    await session.run(session.importSql(path))
  }
}

/**
 * A click opens the table here and clears the selection.
 * Shift-click selects a range. Cmd-click (Ctrl on Windows and Linux) adds or removes one table.
 */
function pick(table: TableRef, event: MouseEvent): void {
  const name = label(table)
  if (event.shiftKey) {
    selection.range(shownNames(), name)
  } else if (shortcuts.hasModifier(event)) {
    selection.toggle(name)
  } else {
    selection.start(name)
    props.session.run(props.session.selectTable(table))
  }
}

/** The new tab shows at once. When its connection fails, it goes away and the error shows in this tab. */
function openInNewTab(table: TableRef): void {
  closeMenu()
  props.session.run(manager.openTable(props.session, table))
}

/** Opens the dialog to rename a table. Pass null for the dialog to create one. */
function openDialog(table: TableRef | null): void {
  closeMenu()
  tableDialog.value?.open(table)
}

/** The confirm text of a truncate or a drop of one or many tables and views. */
function confirmText(action: 'Truncate' | 'Drop', tables: TableRef[]): string {
  const noun = TableSelection.noun(tables)
  const one = tables.length === 1
  let effect = one
    ? `The ${noun} and all its data are deleted.`
    : 'The tables and all their data are deleted.'
  if (action === 'Truncate') {
    effect = one ? 'All rows in it are deleted.' : 'All rows in them are deleted.'
  } else if (noun === 'view' || noun === 'views') {
    effect = one
      ? 'The view is deleted. Its tables keep their data.'
      : 'The views are deleted. Their tables keep their data.'
  } else if (noun === 'tables and views') {
    effect = 'The tables and all their data are deleted. The views are deleted too.'
  }
  if (one) {
    return `${action} ${noun} "${label(tables[0])}"? ${effect} This cannot be undone.`
  }
  const names: string[] = []
  for (const table of tables.slice(0, CONFIRM_NAMES)) {
    names.push(label(table))
  }
  const more = tables.length > CONFIRM_NAMES ? ` and ${tables.length - CONFIRM_NAMES} more` : ''
  return `${action} ${tables.length} ${noun}: ${names.join(', ')}${more}? ${effect} This cannot be undone.`
}

/** Asks first. Truncates or drops the tables of the menu only on yes, then clears the selection. */
async function choose(action: 'Truncate' | 'Drop'): Promise<void> {
  const tables = menu.value?.tables ?? []
  closeMenu()
  if (tables.length === 0) {
    return
  }
  const session = props.session
  const sure = await confirm(confirmText(action, tables), {
    title: `${action} ${TableSelection.noun(tables)}`,
    kind: 'warning',
  })
  if (sure) {
    selection.clear()
    await session.run(
      action === 'Drop' ? session.dropTables(tables) : session.truncateTables(tables),
    )
  }
}

/** True when the menu acts on only one table, so the actions for one table can be shown. */
function single(): boolean {
  return (menu.value?.tables.length ?? 0) <= 1
}

/** True when the menu acts on at least one view. Views cannot be truncated or get rows. */
function hasView(): boolean {
  for (const table of menu.value?.tables ?? []) {
    if (table.kind === 'view') {
      return true
    }
  }
  return false
}

/** MySQL can be connected without a database. Then there is nothing to back up. */
function hasNoDatabase(): boolean {
  return props.session.config?.driver === 'mysql' && props.session.config.database === ''
}

/** The database name. SQLite has none, so it is the file name. */
function databaseLabel(): string {
  const config = props.session.config
  return config?.driver === 'sqlite' ? connection.value : (config?.database ?? '')
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
  const sure = await confirm(
    `Back up database "${databaseLabel()}" to ${path}? A large database can take a few minutes.`,
    { title: 'Back up database', kind: 'info' },
  )
  if (sure) {
    await session.run(session.backup(path))
  }
}

function changeDatabase(event: Event): void {
  props.session.run(switchDatabase(event.target as HTMLSelectElement))
}

async function switchDatabase(select: HTMLSelectElement): Promise<void> {
  try {
    await props.session.useDatabase(select.value)
  } finally {
    // The user may have kept the unsaved changes. Then show the old database again.
    select.value = props.session.config?.database ?? ''
  }
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
        <template v-for="(text, view) in VIEW_LABELS" :key="view">
          <!-- Users is only for an account that may manage users. -->
          <button
            v-if="view !== 'users' || session.canManageUsers"
            :class="{ active: session.view === view }"
            @click="session.view = view"
          >
            <component :is="VIEW_ICONS[view]" :size="12" />
            {{ text }}
          </button>
        </template>
      </div>
      <span class="spacer" />
      <EnvBadge :name="session.environment" />
      <span class="status-dot" />
      <span class="connection">{{ connection }}</span>
      <button class="icon" title="Refresh tables" @click="session.run(session.loadTables())">
        <RefreshCwIcon :size="13" />
      </button>
      <!-- The app cannot back up SQL Server yet. -->
      <button
        v-if="session.config?.driver !== 'mssql'"
        class="icon"
        :title="backupBlocked ? 'Choose a database first' : 'Back up this database to a SQL file'"
        :disabled="backupBlocked"
        @click="backup"
      >
        <DownloadIcon :size="13" />
      </button>
      <button
        class="icon"
        :title="backupBlocked ? 'Choose a database first' : 'Import a SQL file into this database'"
        :disabled="backupBlocked"
        @click="importSql"
      >
        <UploadIcon :size="13" />
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
        <div class="tables" @click="clickList" @contextmenu.prevent="openMenu($event, null)">
          <template v-for="group in groups" :key="group.title">
            <div class="section">
              <span>{{ group.title }}</span>
              <button
                v-if="group.title === 'Tables'"
                class="icon add"
                :title="backupBlocked ? 'Choose a database first' : 'New table'"
                :disabled="backupBlocked"
                @click="openDialog(null)"
              >
                <PlusIcon :size="11" />
              </button>
              <span class="count">{{ group.items.length }}</span>
            </div>
            <ul class="list">
              <li
                v-for="table in group.items"
                :key="label(table)"
                :class="{ selected: session.isOpen(table), picked: selection.has(label(table)) }"
                :title="label(table)"
                @click="pick(table, $event)"
                @click.middle="openInNewTab(table)"
                @contextmenu.prevent.stop="openMenu($event, table)"
              >
                <component :is="group.icon" :size="12" />
                <span class="name">{{ label(table) }}</span>
              </li>
            </ul>
          </template>
          <button class="section toggle" @click="toggle('triggers')">
            <span
              ><ChevronRightIcon :class="{ open: !collapsed.triggers }" :size="10" />Triggers</span
            >
            <span class="count">{{ triggers.length }}</span>
          </button>
          <ul v-if="!collapsed.triggers" class="list">
            <li
              v-for="object in triggers"
              :key="objectTitle(object)"
              :class="{ selected: session.shows(object) }"
              :title="objectTitle(object)"
              @click="showDefinition(object)"
              @contextmenu.prevent.stop="openObjectMenu($event, object)"
            >
              <ZapIcon :size="12" />
              <span class="name">{{ objectLabel(object) }}</span>
            </li>
          </ul>
          <!-- SQLite has no stored procedures or functions. -->
          <template v-if="session.config?.driver !== 'sqlite'">
            <button class="section toggle" @click="toggle('routines')">
              <span
                ><ChevronRightIcon
                  :class="{ open: !collapsed.routines }"
                  :size="10"
                />Routines</span
              >
              <span class="count">{{ routines.length }}</span>
            </button>
            <ul v-if="!collapsed.routines" class="list">
              <li
                v-for="object in routines"
                :key="objectTitle(object)"
                :class="{ selected: session.shows(object) }"
                :title="objectTitle(object)"
                @click="showDefinition(object)"
                @contextmenu.prevent.stop="openObjectMenu($event, object)"
              >
                <SquareFunctionIcon :size="12" />
                <span class="name">{{ objectLabel(object) }}</span>
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
        <PaneResizer size="sidebarWidth" />
      </aside>

      <main class="main">
        <!-- Only hidden, never removed. So the editor keeps its undo history and height. -->
        <div v-show="session.view === 'query'" class="query">
          <QueryView :session="session" />
        </div>
        <template v-if="session.view !== 'query'">
          <!-- The Users view needs no table. It has its own bottom bar for errors. -->
          <UsersView v-if="session.view === 'users'" :session="session" />
          <!-- The diagram shows all tables. It has its own bottom bar too. -->
          <DiagramView v-else-if="session.view === 'diagram'" :session="session" />
          <!-- No table is open, so no view with a bottom bar. Show the error or the status here. -->
          <DefinitionView v-else-if="session.definition" :session="session" />
          <div v-else-if="!session.page" class="empty">
            <span v-if="session.error" class="error failure">{{ session.error }}</span>
            <template v-else-if="session.busy">
              <LoaderCircleIcon :size="13" class="spin" />
              Loading…
            </template>
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
        <template v-if="!menu.table">
          <button role="menuitem" @click.stop="openDialog(null)">New table…</button>
          <span class="menu-line" />
          <div class="has-submenu" @mouseenter="showSubmenu('import')" @mouseleave="hideSubmenu">
            <button
              role="menuitem"
              aria-haspopup="menu"
              :aria-expanded="submenu === 'import'"
              :class="{ open: submenu === 'import' }"
              @click.stop="showSubmenu('import')"
            >
              Import
              <ChevronRightIcon class="arrow" :size="12" />
            </button>
            <div
              v-if="submenu === 'import'"
              class="menu submenu"
              :class="{ flip: menu.flip }"
              role="menu"
            >
              <button role="menuitem" @click.stop="importSql">Import SQL File…</button>
            </div>
          </div>
        </template>
        <template v-else>
          <button v-if="single()" role="menuitem" @click.stop="openInNewTab(menu.table)">
            Open in new tab
          </button>
          <button
            v-if="single() && menu.table.kind !== 'view'"
            role="menuitem"
            @click.stop="openDialog(menu.table)"
          >
            Rename table…
          </button>
          <button
            v-if="single() && menu.table.kind === 'view'"
            role="menuitem"
            @click.stop="showDefinition(Session.viewObject(menu.table))"
          >
            Show definition
          </button>
          <span v-if="single()" class="menu-line" />
          <div class="has-submenu" @mouseenter="showSubmenu('export')" @mouseleave="hideSubmenu">
            <button
              role="menuitem"
              aria-haspopup="menu"
              :aria-expanded="submenu === 'export'"
              :class="{ open: submenu === 'export' }"
              @click.stop="showSubmenu('export')"
            >
              Export
              <ChevronRightIcon class="arrow" :size="12" />
            </button>
            <div
              v-if="submenu === 'export'"
              class="menu submenu"
              :class="{ flip: menu.flip }"
              role="menu"
            >
              <button role="menuitem" @click.stop="exportTables(menu.tables)">
                Export Tables…
                <span class="hint">{{ shortcuts.hint('E', true) }}</span>
              </button>
              <button v-if="single()" role="menuitem" @click.stop="exportColumns(menu.table)">
                Export this Table with Column Selection…
              </button>
            </div>
          </div>
          <div
            v-if="single() && menu.table.kind !== 'view'"
            class="has-submenu"
            @mouseenter="showSubmenu('import')"
            @mouseleave="hideSubmenu"
          >
            <button
              role="menuitem"
              aria-haspopup="menu"
              :aria-expanded="submenu === 'import'"
              :class="{ open: submenu === 'import' }"
              @click.stop="showSubmenu('import')"
            >
              Import
              <ChevronRightIcon class="arrow" :size="12" />
            </button>
            <div
              v-if="submenu === 'import'"
              class="menu submenu"
              :class="{ flip: menu.flip }"
              role="menu"
            >
              <button role="menuitem" @click.stop="importCsv(menu.table)">
                Import CSV into this Table…
              </button>
            </div>
          </div>
          <span class="menu-line" />
          <button v-if="!hasView()" class="danger" role="menuitem" @click.stop="choose('Truncate')">
            Truncate {{ single() ? '' : `${menu.tables.length} `
            }}{{ TableSelection.noun(menu.tables) }}…
          </button>
          <button class="danger" role="menuitem" @click.stop="choose('Drop')">
            Drop {{ single() ? '' : `${menu.tables.length} `
            }}{{ TableSelection.noun(menu.tables) }}…
          </button>
        </template>
      </div>
    </div>
    <ContextMenu v-if="objectMenu" :x="objectMenu.x" :y="objectMenu.y" @close="closeObjectMenu">
      <button role="menuitem" @click="showDefinition(objectMenu.object)">Show definition</button>
      <button role="menuitem" @click="session.run(editObject(objectMenu.object))">
        Edit in Query view
      </button>
      <span class="menu-line" />
      <button class="danger" role="menuitem" @click="dropObject(objectMenu.object)">
        Drop {{ OBJECT_KIND_LABELS[objectMenu.object.kind].toLowerCase() }}…
      </button>
    </ContextMenu>
    <TableDialog ref="tableDialog" :session="session" />
    <ExportDialog ref="exportDialog" :session="session" />
    <ImportDialog ref="importDialog" :session="session" />
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

/* The left padding is the side margin of the search box in the sidebar. */
.toolbar {
  gap: 10px;
  height: 40px;
  padding: 0 10px 0 8px;
}

/*
  A real select. The two icons lie over it and let clicks through.
  It is as wide as the search box below it: the sidebar without its two 8px
  margins and its 1px border. So both boxes have the same left and right edge.
*/
.database {
  position: relative;
  display: flex;
  flex: none;
  align-items: center;
  justify-content: space-between;
  width: calc(var(--sidebar-fit) - 17px);
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
  position: relative;
  flex: none;
  width: var(--sidebar-fit);
  background: var(--sidebar);
  border-right: 1px solid var(--border);
}

.tables {
  flex: 1;
  overflow: auto;
}

/* The small "+" after the title "Tables". The count stays at the right edge. */
.section .add {
  width: 16px;
  height: 14px;
  margin: -1px auto -1px 4px;
  color: var(--text-2);
}

.section .add:hover:enabled {
  color: var(--text);
}

/* The title of the trigger and routine groups. A click opens or closes the group. */
.section.toggle {
  width: 100%;
  height: auto;
  border: 0;
  border-radius: 0;
  background: none;
  text-align: left;
}

.section.toggle span:first-child {
  display: flex;
  gap: 3px;
  align-items: center;
}

.section.toggle svg {
  transition: transform 0.1s;
}

.section.toggle svg.open {
  transform: rotate(90deg);
}

/* A table in the selection. It differs from the open table, which has the full accent colour. */
.list li.picked:not(.selected) {
  background: var(--picked);
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
  color: var(--text);
  font-size: 12px;
  font-weight: 400;
}

.menu button.danger {
  color: var(--danger);
}

.menu-line {
  height: 1px;
  margin: 4px 0;
  background: var(--border);
}

/* The Export and Import items and their submenus at the side. */
/* It reaches over the padding and border of the menu, so the mouse can cross to the submenu. */
.has-submenu {
  position: relative;
  display: flex;
  flex-direction: column;
  margin: 0 -6px;
  padding: 0 6px;
}

.menu .arrow {
  margin-left: auto;
}

.menu.submenu {
  position: absolute;
  top: -6px;
  left: 100%;
  width: max-content;
}

.menu.submenu.flip {
  right: 100%;
  left: auto;
}

/* The shortcut at the right end of an item, like in a macOS menu. */
.menu .hint {
  margin-left: auto;
  padding-left: 24px;
  opacity: 0.6;
}

.menu button.open,
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
