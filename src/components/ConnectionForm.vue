<script setup lang="ts">
import { computed, nextTick, reactive, ref, useTemplateRef, watch } from 'vue'
import {
  ChevronRightIcon,
  CircleAlertIcon,
  CircleCheckIcon,
  DownloadIcon,
  FolderPlusIcon,
  LoaderCircleIcon,
  PlusIcon,
  SearchIcon,
  UploadIcon,
  XIcon,
} from 'lucide-vue-next'
import { confirm, open, save } from '@tauri-apps/plugin-dialog'
import {
  DEFAULT_CONFIGS,
  DEFAULT_SSH_CONFIG,
  DEFAULT_SSL_CONFIG,
  DRIVER_LABELS,
  SSL_MODE_LABELS,
} from '../constants'
import {
  DEFAULT_ENVIRONMENT,
  ENVIRONMENT_HINT,
  ENVIRONMENT_KINDS,
  ENVIRONMENT_MAX_LENGTH,
} from '../constants/environment'
import type { EnvironmentKind } from '../constants/environment'
import { CONNECTIONS_FILE_FILTERS, CONNECTIONS_FILE_NAME } from '../constants/transfer'
import { DRAG_START_DISTANCE } from '../constants/folders'
import {
  KEYCHAIN_DELETE_NOTE,
  KEYCHAIN_READ_NOTE,
  KEYCHAIN_WRITE_NOTE,
} from '../constants/keychain'
import { ConnectionString } from '../services/ConnectionString'
import { ConnectionTransfer } from '../services/ConnectionTransfer'
import { DriverChange } from '../services/DriverChange'
import { DropPlan } from '../services/DropPlan'
import type { Drop } from '../services/DropPlan'
import { Environment } from '../services/Environment'
import type { Favorite, FavoriteSection, Folder } from '../services/FavoriteStore'
import { CONNECTION_TRANSFER, FAVORITE_STORE, injectRequired } from '../services/keys'
import type { Session } from '../services/Session'
import type { ConnectionConfig, Driver, SslConfig, SslMode } from '../types'
import ConnectionInfo from './ConnectionInfo.vue'
import ContextMenu from './ContextMenu.vue'
import DbBadge from './DbBadge.vue'
import EnvBadge from './EnvBadge.vue'
import FolderDialog from './FolderDialog.vue'
import FolderIcon from './FolderIcon.vue'
import PaneResizer from './PaneResizer.vue'

const props = defineProps<{ session: Session }>()
const store = injectRequired(FAVORITE_STORE)
const transfer = injectRequired(CONNECTION_TRANSFER)
const folderDialog = useTemplateRef<InstanceType<typeof FolderDialog>>('folderDialog')
const connectButton = useTemplateRef<HTMLButtonElement>('connectButton')

// `id` is the id of the loaded favorite. '' means a new connection.
// `folderId` is the folder of the connection. '' means no folder.
const form = reactive({ id: '', name: '', folderId: '', config: { ...DEFAULT_CONFIGS.mysql } })

/**
 * True shows the inputs. False shows a saved connection as plain text.
 * A new connection is always in edit mode.
 */
const editing = ref(true)

/** The environment switch. `custom` is the typed name. It counts only for the kind `custom`. */
const env = reactive({ kind: DEFAULT_ENVIRONMENT as EnvironmentKind, custom: '' })

/**
 * A calm note about the keychain. It is not an error: the action itself worked.
 * `detail` is the raw keychain text. It is shown only as a tooltip.
 */
const note = reactive({ text: '', detail: '' })
/** True while a connect runs. The form then says "Connecting…", not only "Working…". */
const connecting = ref(false)

/**
 * The result of the Test button. `error` is '' when the test worked.
 * `done` is false while no result is shown. Any change in the form clears it.
 */
const test = reactive({ running: false, done: false, error: '' })
watch(form, clearTest)

/** The pasted connection string. It can hold a password, so it is cleared after use. */
const pasted = ref('')

const search = ref('')
const rows = computed(shownRows)

/** The right-click menu that is open now. It is for a folder or for a connection. */
const menu = ref<{ x: number; y: number; folder: Folder | null; favorite: Favorite | null } | null>(
  null,
)

/**
 * The row under the mouse button: a connection or a folder. `moved` turns true when
 * the drag starts. `x` and `y` are where the drag started. `nowX` and `nowY` are where
 * the mouse is now. `drop` says what a drop here would do. null = no drop place.
 */
const drag = reactive({
  favorite: null as Favorite | null,
  folder: null as Folder | null,
  moved: false,
  x: 0,
  y: 0,
  nowX: 0,
  nowY: 0,
  drop: null as Drop | null,
})

/** One row of the list. A folder row has no `favorite` and '' as `favoriteId`. */
interface Row {
  key: string
  section: FavoriteSection
  favorite: Favorite | null
  /** '' = the top level. */
  folderId: string
  favoriteId: string
}

/**
 * The list as flat rows. The slide animation needs every row as a direct child
 * of the list, each with its own key. A row keeps its key when it changes folder.
 */
function shownRows(): Row[] {
  const list: Row[] = []
  for (const section of store.sections(search.value)) {
    const folderId = section.folder?.id ?? ''
    if (section.folder) {
      // The prefix keeps a folder key apart from every connection id.
      list.push({ key: `folder:${folderId}`, section, favorite: null, folderId, favoriteId: '' })
    }
    if (section.open) {
      for (const favorite of section.favorites) {
        list.push({ key: favorite.id, section, favorite, folderId, favoriteId: favorite.id })
      }
    }
  }
  return list
}

/**
 * A click opens or closes the folder. Not while searching: then every shown folder is open.
 * The click at the end of a drag does nothing.
 */
function toggleFolder(folder: Folder): void {
  if (!drag.moved && !search.value.trim()) {
    store.toggleFolder(folder.id)
  }
}

/** Opens the menu of a folder. */
function openFolderMenu(event: MouseEvent, folder: Folder): void {
  menu.value = { x: event.clientX, y: event.clientY, folder, favorite: null }
}

/** Opens the menu of a connection. */
function openFavoriteMenu(event: MouseEvent, favorite: Favorite): void {
  menu.value = { x: event.clientX, y: event.clientY, folder: null, favorite }
}

function closeMenu(): void {
  menu.value = null
}

/** Moves a connection into a folder. '' = out of its folder. */
function moveTo(favorite: Favorite, folderId: string): void {
  store.move(favorite.id, folderId)
  if (form.id === favorite.id) {
    form.folderId = store.folderOf(favorite)
  }
}

/** Asks first. The connections of the folder are never deleted. */
async function deleteFolder(folder: Folder): Promise<void> {
  const count = store.inFolder(folder.id).length
  let effect = ''
  if (count === 1) {
    effect = ' Its 1 connection is not deleted. It moves back to the top of the list.'
  } else if (count > 1) {
    effect = ` Its ${count} connections are not deleted. They move back to the top of the list.`
  }
  const sure = await confirm(`Delete folder "${folder.name}"?${effect}`, {
    title: 'Delete folder',
    kind: 'warning',
  })
  if (sure) {
    store.removeFolder(folder.id)
    if (form.folderId === folder.id) {
      form.folderId = ''
    }
  }
}

/**
 * The mouse button goes down on a row. The row takes all moves from now on.
 * One of `favorite` and `folder` is set: the thing that is dragged.
 * This uses pointer events and not HTML drag and drop: the Tauri window takes the
 * HTML drag events for file drops, so they do not reach the page on every platform.
 */
function startDrag(event: PointerEvent, favorite: Favorite | null, folder: Folder | null): void {
  cancelDrag()
  drag.moved = false
  // The delete button keeps its own click.
  if ((event.target as HTMLElement).closest('button')) {
    return
  }
  ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
  drag.favorite = favorite
  drag.folder = folder
  drag.x = event.clientX
  drag.y = event.clientY
}

/** Finds the drop place under the mouse. A small move is still a click. */
function moveDrag(event: PointerEvent): void {
  if (!drag.favorite && !drag.folder) {
    return
  }
  const distance = Math.hypot(event.clientX - drag.x, event.clientY - drag.y)
  if (!drag.moved && distance < DRAG_START_DISTANCE) {
    return
  }
  drag.moved = true
  drag.nowX = event.clientX
  drag.nowY = event.clientY
  // ponytail: the list does not scroll by itself during a drag. The mouse wheel still works.
  const under = document.elementFromPoint(event.clientX, event.clientY)
  if (!under?.closest('[data-drop]')) {
    drag.drop = null
    return
  }
  // No row under the mouse: the empty part of the list, or the "no folder" box.
  const element = under.closest<HTMLElement>('[data-row]')
  let index = -1
  let upper = false
  if (element) {
    const box = element.getBoundingClientRect()
    index = Number(element.dataset.row)
    upper = event.clientY < box.top + box.height / 2
  }
  drag.drop = DropPlan.decide(rows.value, !!drag.folder, index, upper)
}

/** The mouse button goes up: the dragged row moves to the place under the mouse. */
function endDrag(): void {
  const favorite = drag.favorite
  const folder = drag.folder
  const drop = drag.drop
  cancelDrag()
  if (!drop) {
    return
  }
  // The store wants "before this id". "After" is "before the next one" of the full list.
  if (favorite) {
    const group = store.inFolder(drop.folderId)
    const beforeId = drop.after ? DropPlan.next(group, drop.anchorId) : drop.anchorId
    store.place(favorite.id, drop.folderId, beforeId)
    if (form.id === favorite.id) {
      form.folderId = store.folderOf(favorite)
    }
  } else if (folder) {
    const beforeId = drop.after ? DropPlan.next(store.folders, drop.anchorId) : drop.anchorId
    store.placeFolder(folder.id, beforeId)
  }
}

/**
 * Ends the drag with no drop. It also runs when the row loses the mouse, and when a row
 * leaves the list in the middle of a drag (the search text changed). So the chip
 * that follows the mouse can never stay on the screen.
 */
function cancelDrag(): void {
  drag.favorite = null
  drag.folder = null
  drag.drop = null
}

/** The class that draws the line where the dragged row would go. '' = no line here. */
function lineClass(index: number): string {
  if (drag.drop?.line !== index) {
    return ''
  }
  return drag.drop.lineAfter ? 'line-after' : 'line-before'
}

/** A click loads the connection. The click at the end of a drag does nothing. */
function clickRow(favorite: Favorite): void {
  if (!drag.moved) {
    void runThenFocus(load(favorite))
  }
}

/**
 * Runs the work. Then, in the text view, the focus goes to the Connect button.
 * The text view has no input, so without this Enter would not connect.
 */
async function runThenFocus(work: Promise<unknown>): Promise<void> {
  await props.session.run(work)
  await nextTick()
  if (!editing.value) {
    connectButton.value?.focus()
  }
}

/** The name of the folder in the form. '' when it has none. */
function folderName(): string {
  for (const folder of store.folders) {
    if (folder.id === form.folderId) {
      return folder.name
    }
  }
  return ''
}

/** Shows the note when the keychain failed. Clears it when `detail` is ''. */
function setNote(text: string, detail: string): void {
  note.text = detail ? text : ''
  note.detail = detail
}

/** The environment name to save and to connect with. An empty custom name is `local`. */
function environment(): string {
  return Environment.orLocal(env.kind === 'custom' ? env.custom : env.kind)
}

/** Sets the switch from a name. A built-in name like `prod` picks its own button. */
function setEnvironment(name: string): void {
  const clean = Environment.orLocal(name)
  env.kind = Environment.kind(clean)
  env.custom = env.kind === 'custom' ? clean : ''
}

/** Cleans the custom name while the user types. Bad characters are dropped. */
function typeCustom(event: Event): void {
  const input = event.target as HTMLInputElement
  env.custom = Environment.clean(input.value)
  input.value = env.custom
}

function reset(): void {
  setNote('', '')
  editing.value = true
  form.id = ''
  form.name = ''
  form.folderId = ''
  setEnvironment(DEFAULT_ENVIRONMENT)
  form.config = { ...DEFAULT_CONFIGS.mysql }
}

function setDriver(driver: Driver): void {
  form.config = DriverChange.apply(form.config, driver)
}

/**
 * Fills the fields from the pasted connection string. It does not save or connect.
 * The name, the folder and the environment stay as they are.
 */
function fillFromString(): void {
  const session = props.session
  try {
    form.config = ConnectionString.parse(pasted.value)
    pasted.value = ''
    session.error = ''
  } catch (error) {
    session.error = (error as Error).message
  }
}

/** Turns the SSH fields on or off. */
function toggleSsh(): void {
  form.config.ssh = form.config.ssh ? null : { ...DEFAULT_SSH_CONFIG }
}

/** The SSL mode in the form. A connection saved before SSL support has none: then `prefer`. */
function sslMode(): SslMode {
  return form.config.ssl?.mode ?? DEFAULT_SSL_CONFIG.mode
}

function setSslMode(event: Event): void {
  const mode = (event.target as HTMLSelectElement).value as SslMode
  form.config.ssl = { ...(form.config.ssl ?? DEFAULT_SSL_CONFIG), mode }
}

/**
 * The certificate files show only for the modes that need them, so the form stays short.
 * SQL Server reads the CA file only when it checks the certificate.
 */
function showsSslFiles(): boolean {
  const mode = sslMode()
  if (form.config.driver === 'mssql') {
    return mode === 'verifyCa' || mode === 'verifyFull'
  }
  return mode !== 'off' && mode !== 'prefer'
}

/** Picks a certificate or key file for one SSL field. */
async function browseSsl(field: keyof Omit<SslConfig, 'mode'>): Promise<void> {
  const path = await open({ multiple: false, directory: false })
  if (typeof path === 'string' && form.config.ssl) {
    form.config.ssl[field] = path
  }
}

/** The form values as a clean config. An empty port field becomes 0. */
function currentConfig(): ConnectionConfig {
  const ssh = form.config.ssh
    ? { ...form.config.ssh, port: Number(form.config.ssh.port) || DEFAULT_SSH_CONFIG.port }
    : null
  return { ...form.config, port: Number(form.config.port) || 0, ssh }
}

/** The tab title and the default favorite name. */
function title(): string {
  if (form.name.trim()) {
    return form.name.trim()
  }
  if (form.config.driver === 'sqlite') {
    return form.config.database.split(/[\\/]/).pop() || 'SQLite'
  }
  return `${form.config.user}@${form.config.host}`
}

/**
 * Shows the connection as text. Returns false when the keychain did not give out
 * a saved password. Then the inputs open, so the user can type the password.
 */
async function load(favorite: Favorite): Promise<boolean> {
  const loaded = await store.fullConfig(favorite)
  form.id = favorite.id
  form.name = favorite.name
  form.folderId = store.folderOf(favorite)
  setEnvironment(favorite.environment)
  form.config = loaded.config
  // A pasted string can hold a password. It must not wait behind the text view.
  pasted.value = ''
  editing.value = !!loaded.keychainError
  setNote(KEYCHAIN_READ_NOTE, loaded.keychainError)
  return !loaded.keychainError
}

async function connect(): Promise<void> {
  setNote('', '')
  connecting.value = true
  try {
    await props.session.connect(currentConfig(), title(), environment())
  } finally {
    connecting.value = false
  }
}

/**
 * Opens a connection with the values in the form, checks it and closes it again.
 * A saved password is in the form already, because `load` puts it there.
 */
async function testConnection(): Promise<void> {
  clearTest()
  test.running = true
  try {
    await props.session.testConnection(currentConfig())
  } catch (error) {
    test.error = String(error)
  } finally {
    test.running = false
    test.done = true
  }
}

function clearTest(): void {
  test.done = false
  test.error = ''
}

/** Does not connect with an empty password. The user types it first. */
async function loadAndConnect(favorite: Favorite): Promise<void> {
  if (await load(favorite)) {
    await connect()
  }
}

async function saveFavorite(): Promise<void> {
  form.id = form.id || crypto.randomUUID()
  form.name = title()
  // A custom name like `prod` moves the switch to its own button.
  setEnvironment(environment())
  const failure = await store.save({
    id: form.id,
    name: form.name,
    environment: environment(),
    folderId: form.folderId,
    config: currentConfig(),
  })
  setNote(KEYCHAIN_WRITE_NOTE, failure)
  editing.value = false
}

/** Throws the edits away: loads the saved connection again and shows it as text. */
async function cancelEdit(): Promise<void> {
  const saved = store.byId(form.id)
  if (!saved) {
    reset()
    return
  }
  await load(saved)
  editing.value = false
}

/** Makes a copy and loads it into the form, so the user can rename it right away. */
async function duplicateFavorite(favorite: Favorite): Promise<void> {
  const made = await store.duplicate(favorite)
  await load(made.favorite)
  editing.value = true
  // The copy has no saved password then. This note says so.
  if (made.keychainError) {
    setNote(KEYCHAIN_WRITE_NOTE, made.keychainError)
  }
}

/** Asks first. The saved password goes away with the connection. */
async function removeFavorite(favorite: Favorite): Promise<void> {
  const sure = await confirm(
    `Delete the saved connection "${favorite.name}"? Its saved password is removed too. This cannot be undone.`,
    { title: 'Delete connection', kind: 'warning' },
  )
  if (sure) {
    const failure = await store.remove(favorite.id)
    // The form must not keep showing a connection that is gone.
    if (form.id === favorite.id) {
      reset()
    }
    setNote(KEYCHAIN_DELETE_NOTE, failure)
  }
}

/** Writes all saved connections to a file the user picks. Passwords are not written. */
async function exportFavorites(): Promise<void> {
  const path = await save({ defaultPath: CONNECTIONS_FILE_NAME, filters: CONNECTIONS_FILE_FILTERS })
  if (path) {
    const count = await transfer.exportTo(path)
    showNote(`Exported ${connections(count)}. Passwords are not in the file.`)
  }
}

/** Writes one saved connection to a file the user picks. Passwords are not written. */
async function exportFavorite(favorite: Favorite): Promise<void> {
  const defaultPath = ConnectionTransfer.fileName(favorite.name)
  const path = await save({ defaultPath, filters: CONNECTIONS_FILE_FILTERS })
  if (path) {
    await transfer.exportTo(path, [favorite])
    showNote(`Exported ${favorite.name}. Passwords are not in the file.`)
  }
}

/** Adds the connections of a file the user picks. Errors show in the form banner. */
async function importFavorites(): Promise<void> {
  const path = await open({ multiple: false, directory: false, filters: CONNECTIONS_FILE_FILTERS })
  if (typeof path === 'string') {
    const count = await transfer.importFrom(path)
    showNote(`Imported ${connections(count)}`)
  }
}

/** `1 connection`, `3 connections`. */
function connections(count: number): string {
  return count === 1 ? '1 connection' : `${count} connections`
}

/** Shows a short status line in the form banner. */
function showNote(text: string): void {
  note.text = text
  note.detail = ''
}

async function browse(): Promise<void> {
  const path = await open({ multiple: false, directory: false })
  if (typeof path === 'string') {
    form.config.database = path
  }
}

async function browseKey(): Promise<void> {
  const path = await open({ multiple: false, directory: false })
  if (typeof path === 'string' && form.config.ssh) {
    form.config.ssh.keyPath = path
  }
}
</script>

<template>
  <div class="connect">
    <aside class="favorites">
      <div class="search">
        <SearchIcon :size="11" />
        <input v-model="search" placeholder="Filter connections" spellcheck="false" />
      </div>
      <!--
        `data-drop` marks the places that take a dragged row. `data-row` is the index of a row.
        Rows slide to their new place when the list changes.
      -->
      <TransitionGroup tag="ul" class="list" name="slide" data-drop @before-leave="cancelDrag">
        <template v-for="(row, index) in rows" :key="row.key">
          <li
            v-if="row.favorite"
            :class="[
              {
                selected: row.favorite.id === form.id,
                child: row.section.folder,
                dragged: drag.moved && drag.favorite === row.favorite,
              },
              lineClass(index),
            ]"
            :data-row="index"
            @click="clickRow(row.favorite)"
            @dblclick="session.run(loadAndConnect(row.favorite))"
            @contextmenu.prevent="openFavoriteMenu($event, row.favorite)"
            @pointerdown.left="startDrag($event, row.favorite, null)"
            @pointermove="moveDrag"
            @pointerup="endDrag"
            @pointercancel="cancelDrag"
            @lostpointercapture="cancelDrag"
          >
            <DbBadge :driver="row.favorite.config.driver" />
            <span class="name">{{ row.favorite.name }}</span>
            <EnvBadge class="env" :name="row.favorite.environment" />
            <button
              type="button"
              class="icon remove"
              title="Delete connection"
              @click.stop="session.run(removeFavorite(row.favorite))"
            >
              <XIcon :size="11" />
            </button>
          </li>
          <li
            v-else-if="row.section.folder"
            class="folder"
            :class="[
              {
                drop: drag.drop?.into === row.folderId,
                dragged: drag.moved && drag.folder === row.section.folder,
              },
              lineClass(index),
            ]"
            :data-row="index"
            :title="row.section.folder.name"
            @click="toggleFolder(row.section.folder)"
            @contextmenu.prevent="openFolderMenu($event, row.section.folder)"
            @pointerdown.left="startDrag($event, null, row.section.folder)"
            @pointermove="moveDrag"
            @pointerup="endDrag"
            @pointercancel="cancelDrag"
            @lostpointercapture="cancelDrag"
          >
            <FolderIcon
              :icon="row.section.folder.icon"
              :color="row.section.folder.color"
              :size="13"
            />
            <span class="name">{{ row.section.folder.name }}</span>
            <span class="count">{{ row.section.favorites.length }}</span>
            <ChevronRightIcon class="chevron" :class="{ open: row.section.open }" :size="10" />
          </li>
        </template>
      </TransitionGroup>
      <!-- The dragged row follows the mouse. It must not take the mouse: see `moveDrag`. -->
      <div
        v-if="drag.moved && (drag.favorite || drag.folder)"
        class="ghost"
        :style="{ transform: `translate(${drag.nowX + 12}px, ${drag.nowY + 10}px)` }"
      >
        <DbBadge v-if="drag.favorite" :driver="drag.favorite.config.driver" />
        <FolderIcon
          v-else-if="drag.folder"
          :icon="drag.folder.icon"
          :color="drag.folder.color"
          :size="13"
        />
        <span class="name">{{ (drag.favorite ?? drag.folder)?.name }}</span>
      </div>
      <!-- Shown while a connection of a folder is dragged. -->
      <div
        v-if="drag.moved && drag.favorite && store.folderOf(drag.favorite)"
        class="drop-top"
        :class="{ drop: drag.drop?.into === '' }"
        data-drop
      >
        Drop here for no folder
      </div>
      <div class="footer">
        <button type="button" class="sidebar-footer" @click="reset">
          <PlusIcon :size="12" />
          <span class="label">New connection</span>
        </button>
        <button
          type="button"
          class="sidebar-footer"
          title="New folder"
          @click="folderDialog?.open(null)"
        >
          <FolderPlusIcon :size="13" />
        </button>
        <button
          type="button"
          class="sidebar-footer"
          title="Import connections"
          @click="session.run(importFavorites())"
        >
          <DownloadIcon :size="13" />
        </button>
        <button
          type="button"
          class="sidebar-footer"
          title="Export connections"
          @click="session.run(exportFavorites())"
        >
          <UploadIcon :size="13" />
        </button>
      </div>
      <PaneResizer size="sidebarWidth" />
    </aside>

    <div class="main">
      <form class="form" @submit.prevent="session.run(connect())">
        <ConnectionInfo
          v-if="!editing"
          :name="form.name"
          :environment="environment()"
          :folder-name="folderName()"
          :config="form.config"
        />
        <template v-else>
          <header>
            <h1>{{ form.id ? 'Edit connection' : 'New connection' }}</h1>
            <p v-if="!form.id">Pick a database type, then fill in the details.</p>
          </header>

          <!-- Enter here fills the form. It must not submit the form, which would connect. -->
          <div class="row">
            <input
              v-model="pasted"
              class="mono"
              placeholder="Paste a URL, like mysql://user:pass@host/db"
              aria-label="Connection string"
              autocomplete="off"
              autocorrect="off"
              autocapitalize="off"
              spellcheck="false"
              @keydown.enter.prevent="fillFromString"
            />
            <button type="button" @click="fillFromString">Fill</button>
          </div>

          <div class="segmented">
            <button
              v-for="(label, driver) in DRIVER_LABELS"
              :key="driver"
              type="button"
              :class="{ active: form.config.driver === driver }"
              @click="setDriver(driver)"
            >
              <DbBadge :driver="driver" />
              {{ label }}
            </button>
          </div>

          <div class="fields">
            <div class="row">
              <label>Name</label>
              <input v-model="form.name" placeholder="Optional" />
            </div>
            <div class="row">
              <label>Environment</label>
              <div class="segmented env-switch">
                <button
                  v-for="kind in ENVIRONMENT_KINDS"
                  :key="kind"
                  type="button"
                  :class="{ active: env.kind === kind }"
                  @click="env.kind = kind"
                >
                  {{ kind }}
                </button>
              </div>
              <input
                v-if="env.kind === 'custom'"
                :value="env.custom"
                placeholder="name"
                :title="ENVIRONMENT_HINT"
                :maxlength="ENVIRONMENT_MAX_LENGTH"
                autocapitalize="off"
                spellcheck="false"
                @input="typeCustom"
              />
            </div>

            <div class="row">
              <label>Folder</label>
              <select v-model="form.folderId">
                <option value="">No folder</option>
                <option v-for="folder in store.folders" :key="folder.id" :value="folder.id">
                  {{ folder.name }}
                </option>
              </select>
            </div>

            <div v-if="form.config.driver === 'sqlite'" class="row">
              <label>Database file</label>
              <input
                v-model="form.config.database"
                class="mono"
                placeholder="/path/to/database.sqlite"
                spellcheck="false"
              />
              <button type="button" @click="session.run(browse())">Choose…</button>
            </div>
            <template v-else>
              <div class="row">
                <label>Host</label>
                <input
                  v-model="form.config.host"
                  class="mono"
                  autocapitalize="off"
                  spellcheck="false"
                />
                <label class="short">Port</label>
                <input
                  v-model.number="form.config.port"
                  class="port"
                  type="number"
                  min="0"
                  max="65535"
                />
              </div>
              <div class="row">
                <label>Username</label>
                <input
                  v-model="form.config.user"
                  class="mono"
                  autocapitalize="off"
                  spellcheck="false"
                />
              </div>
              <div class="row">
                <label>Password</label>
                <input v-model="form.config.password" type="password" />
              </div>
              <div class="row">
                <label>Database</label>
                <input
                  v-model="form.config.database"
                  autocapitalize="off"
                  spellcheck="false"
                  :placeholder="
                    form.config.driver === 'mysql' ? 'Optional. You can pick one later.' : ''
                  "
                />
              </div>

              <div class="row">
                <label>SSL</label>
                <select :value="sslMode()" @change="setSslMode">
                  <option v-for="(label, mode) in SSL_MODE_LABELS" :key="mode" :value="mode">
                    {{ label }}
                  </option>
                </select>
              </div>
              <template v-if="form.config.ssl && showsSslFiles()">
                <div class="row">
                  <label>CA file</label>
                  <input
                    :value="form.config.ssl.caPath"
                    class="mono"
                    placeholder="Optional. Empty = system CAs."
                    spellcheck="false"
                    readonly
                    @click="session.run(browseSsl('caPath'))"
                  />
                  <button
                    v-if="form.config.ssl.caPath"
                    type="button"
                    class="icon"
                    title="Clear"
                    aria-label="Clear"
                    @click="form.config.ssl.caPath = ''"
                  >
                    <XIcon :size="11" />
                  </button>
                  <button type="button" @click="session.run(browseSsl('caPath'))">Choose…</button>
                </div>
                <template v-if="form.config.driver !== 'mssql'">
                  <div class="row">
                    <label>Client cert</label>
                    <input
                      :value="form.config.ssl.certPath"
                      class="mono"
                      placeholder="Optional"
                      spellcheck="false"
                      readonly
                      @click="session.run(browseSsl('certPath'))"
                    />
                    <button
                      v-if="form.config.ssl.certPath"
                      type="button"
                      class="icon"
                      title="Clear"
                      aria-label="Clear"
                      @click="form.config.ssl.certPath = ''"
                    >
                      <XIcon :size="11" />
                    </button>
                    <button type="button" @click="session.run(browseSsl('certPath'))">
                      Choose…
                    </button>
                  </div>
                  <div class="row">
                    <label>Client key</label>
                    <input
                      :value="form.config.ssl.keyPath"
                      class="mono"
                      placeholder="Optional"
                      spellcheck="false"
                      readonly
                      @click="session.run(browseSsl('keyPath'))"
                    />
                    <button
                      v-if="form.config.ssl.keyPath"
                      type="button"
                      class="icon"
                      title="Clear"
                      aria-label="Clear"
                      @click="form.config.ssl.keyPath = ''"
                    >
                      <XIcon :size="11" />
                    </button>
                    <button type="button" @click="session.run(browseSsl('keyPath'))">
                      Choose…
                    </button>
                  </div>
                </template>
              </template>

              <div class="row">
                <label></label>
                <label class="check">
                  <input type="checkbox" :checked="!!form.config.ssh" @change="toggleSsh" />
                  Connect over SSH
                </label>
              </div>
              <template v-if="form.config.ssh">
                <div class="row">
                  <label>SSH Host</label>
                  <input
                    v-model="form.config.ssh.host"
                    class="mono"
                    autocapitalize="off"
                    spellcheck="false"
                  />
                  <label class="short">Port</label>
                  <input
                    v-model.number="form.config.ssh.port"
                    class="port"
                    type="number"
                    min="1"
                    max="65535"
                  />
                </div>
                <div class="row">
                  <label>SSH User</label>
                  <input
                    v-model="form.config.ssh.user"
                    class="mono"
                    autocapitalize="off"
                    spellcheck="false"
                  />
                </div>
                <div class="row">
                  <label>SSH Password</label>
                  <input
                    v-model="form.config.ssh.password"
                    type="password"
                    placeholder="Or key passphrase. Empty = agent."
                  />
                </div>
                <div class="row">
                  <label>SSH Key</label>
                  <input
                    v-model="form.config.ssh.keyPath"
                    class="mono"
                    placeholder="Optional"
                    spellcheck="false"
                  />
                  <button type="button" @click="session.run(browseKey())">Choose…</button>
                </div>
              </template>
            </template>
          </div>
        </template>

        <div v-if="session.error" class="banner error">
          <CircleAlertIcon :size="13" />
          <span>{{ session.error }}</span>
        </div>
        <div v-else-if="session.busy" class="banner">
          <template v-if="connecting">
            <LoaderCircleIcon :size="13" class="spin" />
            <span>Connecting…</span>
          </template>
          <template v-else>Working…</template>
        </div>
        <div v-else-if="note.text" class="banner note" :title="note.detail">
          <CircleAlertIcon :size="13" />
          <span>{{ note.text }}</span>
        </div>

        <div v-if="test.done" class="banner" :class="test.error ? 'error' : 'success'">
          <CircleAlertIcon v-if="test.error" :size="13" />
          <CircleCheckIcon v-else :size="13" />
          <span>{{ test.error || 'Connection works.' }}</span>
        </div>

        <div class="actions">
          <span class="spacer" />
          <template v-if="editing">
            <button
              type="button"
              :disabled="session.busy || test.running"
              @click="testConnection()"
            >
              <LoaderCircleIcon v-if="test.running" :size="13" class="spin" />
              {{ test.running ? 'Testing…' : 'Test' }}
            </button>
            <button
              v-if="form.id"
              type="button"
              :disabled="session.busy"
              @click="runThenFocus(cancelEdit())"
            >
              Cancel
            </button>
            <button type="button" :disabled="session.busy" @click="runThenFocus(saveFavorite())">
              {{ form.id ? 'Update' : 'Save connection' }}
            </button>
          </template>
          <button v-else type="button" :disabled="session.busy" @click="editing = true">
            Edit
          </button>
          <button ref="connectButton" type="submit" class="primary" :disabled="session.busy">
            {{ form.config.driver === 'sqlite' ? 'Open' : 'Connect' }}
            <span class="hint">↵</span>
          </button>
        </div>
      </form>
    </div>

    <ContextMenu v-if="menu" :x="menu.x" :y="menu.y" @close="closeMenu">
      <template v-if="menu.folder">
        <button role="menuitem" @click="folderDialog?.open(menu.folder)">Edit folder…</button>
        <span class="menu-line" />
        <button class="danger" role="menuitem" @click="session.run(deleteFolder(menu.folder))">
          Delete folder…
        </button>
      </template>
      <template v-else-if="menu.favorite">
        <button role="menuitem" @click="session.run(duplicateFavorite(menu.favorite))">
          Duplicate
        </button>
        <button role="menuitem" @click="session.run(exportFavorite(menu.favorite))">
          Export connection config
        </button>
        <!-- With folders there is always a "Move" item below. -->
        <span v-if="store.folders.length > 0" class="menu-line" />
        <!-- The folder it is in already is left out. -->
        <template v-for="folder in store.folders" :key="folder.id">
          <button
            v-if="store.folderOf(menu.favorite) !== folder.id"
            role="menuitem"
            @click="moveTo(menu.favorite, folder.id)"
          >
            Move to {{ folder.name }}
          </button>
        </template>
        <template v-if="store.folderOf(menu.favorite)">
          <span class="menu-line" />
          <button role="menuitem" @click="moveTo(menu.favorite, '')">Move out of the folder</button>
        </template>
      </template>
    </ContextMenu>
    <FolderDialog ref="folderDialog" />
  </div>
</template>

<style scoped>
.connect {
  display: flex;
  flex: 1;
  min-height: 0;
}

.favorites {
  display: flex;
  flex: none;
  position: relative;
  flex-direction: column;
  width: var(--sidebar-fit);
  background: var(--sidebar);
  border-right: 1px solid var(--border);
}

.list {
  flex: 1;
  overflow: auto;
}

.list li {
  gap: 7px;
  height: 24px;
  cursor: pointer;
}

/* A long environment name is cut, so the connection name keeps its room. */
.list .env {
  flex: 0 1 auto;
  min-width: 0;
  max-width: 72px;
}

/* In a narrow sidebar the environment badge gets smaller before the name does. */
.list li:not(.folder) .name {
  min-width: 40px;
}

/* A connection inside a folder. Its badge lines up with the folder name: 10px padding + 13px icon + 7px gap. */
.list li.child {
  padding-left: 30px;
}

/* The row keeps the mouse during a drag, so its cursor shows for the whole drag. */
.list li.dragged {
  opacity: 0.5;
  cursor: grabbing;
}

.folder {
  font-weight: 600;
}

/* The count sits right after the folder name. A long name is cut, not the count. */
.list li.folder .name {
  flex: 0 1 auto;
  min-width: 0;
}

/* The chevron stays at the right end of a folder row. */
.list li.folder .chevron {
  margin-left: auto;
}

.chevron {
  flex: none;
  transition: transform 0.1s;
}

.chevron.open {
  transform: rotate(90deg);
}

.count {
  flex: none;
  color: var(--text-3);
  font-size: 10px;
  font-weight: 400;
}

/* The copy of the dragged row that follows the mouse. */
.ghost {
  display: flex;
  position: fixed;
  top: 0;
  left: 0;
  z-index: 100;
  gap: 7px;
  align-items: center;
  max-width: 220px;
  height: 24px;
  padding: 0 8px;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 5px;
  box-shadow: 0 6px 20px #00000040;
  white-space: nowrap;
  pointer-events: none;
}

.ghost .name {
  overflow: hidden;
  text-overflow: ellipsis;
}

/* The drop highlight fades in. */
.list li.folder,
.drop-top {
  transition:
    background-color 0.15s,
    box-shadow 0.15s;
}

/* A row that changes place slides there. A new row fades in. A removed row just goes. */
.list li.slide-move {
  transition:
    transform 0.15s,
    background-color 0.15s,
    box-shadow 0.15s;
}

.list li.slide-enter-active {
  transition: opacity 0.1s;
}

.list li.slide-enter-from {
  opacity: 0;
}

@media (prefers-reduced-motion: reduce) {
  .list li,
  .drop-top,
  .chevron {
    transition: none !important;
  }
}

/* The line where the dragged row would go. On the selected row it needs another colour. */
.list li.line-before {
  box-shadow: inset 0 2px 0 var(--line, var(--accent));
}

.list li.line-after {
  box-shadow: inset 0 -2px 0 var(--line, var(--accent));
}

.list li.selected {
  --line: var(--on-accent);
}

/* A thin white ring, drawn with a shadow so the badge keeps its size. */
.list li.selected :deep(.badge) {
  box-shadow: 0 0 0 0.5px var(--on-accent);
}

/* The place a dragged connection would go to. */
.list li.drop,
.drop-top.drop {
  background: var(--stmt-highlight);
  box-shadow: inset 0 0 0 1px var(--accent);
}

.drop-top {
  flex: none;
  margin: 6px 8px;
  padding: 6px 8px;
  overflow: hidden;
  border: 1px dashed var(--border);
  border-radius: 5px;
  color: var(--text-2);
  font-size: 11px;
  text-align: center;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* The last row of the sidebar: "New connection", then small icon buttons (folder, import, export). */
.footer {
  display: flex;
  flex: none;
}

.footer .sidebar-footer:first-child {
  flex: 1;
  width: auto;
  min-width: 0;
}

.footer .label {
  overflow: hidden;
  text-overflow: ellipsis;
}

.footer .sidebar-footer:not(:first-child) {
  justify-content: center;
  width: 32px;
  padding: 0;
  border-left: 1px solid var(--border);
}

/* The delete mark shows only under the mouse. */
.remove {
  visibility: hidden;
  width: 16px;
  height: 16px;
  color: inherit;
}

li:hover .remove {
  visibility: visible;
}

/* `margin: auto` centres the form. A long form can still scroll from its top. */
.main {
  display: flex;
  flex: 1;
  min-width: 0;
  padding: 24px;
  overflow: auto;
}

.form {
  display: flex;
  flex: none;
  flex-direction: column;
  gap: 16px;
  width: 480px;
  margin: auto;
}

h1 {
  margin: 0 0 4px;
  font-size: 17px;
  font-weight: 600;
}

header p {
  margin: 0;
  color: var(--text-2);
}

.segmented {
  height: 32px;
}

.segmented button {
  flex: 1;
  height: 28px;
  border-radius: 5px;
}

/* The small switch inside a form row. */
.segmented.env-switch {
  height: 24px;
  border-radius: 6px;
}

.segmented.env-switch button {
  flex: none;
  height: 20px;
  padding: 0 6px;
  border-radius: 4px;
}

.fields {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.row {
  display: flex;
  gap: 10px;
  align-items: center;
  height: 24px;
}

.row label {
  flex: none;
  width: 92px;
  color: var(--text-2);
}

.row label.short {
  width: auto;
}

.row input,
.row select {
  flex: 1;
  min-width: 0;
}

.row select {
  height: 24px;
  padding: 0 6px;
  border-radius: 5px;
}

.row input.port {
  flex: none;
  width: 72px;
}

.row button {
  height: 24px;
}

.row label.check {
  display: flex;
  gap: 7px;
  align-items: center;
  width: auto;
  color: var(--text);
}

.check input {
  display: grid;
  flex: none;
  place-content: center;
  width: 13px;
  height: 13px;
  margin: 0;
  padding: 0;
  border-radius: 3px;
  appearance: none;
}

.check input:checked {
  background: var(--accent);
  border-color: var(--accent);
}

/* The white check mark: two sides of a small turned box. */
.check input:checked::after {
  width: 3px;
  height: 6px;
  margin-top: -1px;
  border: solid var(--on-accent);
  border-width: 0 1.5px 1.5px 0;
  content: '';
  transform: rotate(45deg);
}

.banner {
  display: flex;
  gap: 8px;
  align-items: flex-start;
  padding: 7px 10px;
  background: var(--header);
  border-radius: 6px;
  color: var(--text-2);
  font-weight: 500;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.banner.error {
  background: var(--deleted);
}

.banner.success {
  color: var(--success-text);
}

.banner.note {
  background: var(--modified);
  color: var(--warn);
}

.banner svg {
  margin-top: 1px;
}

.actions {
  display: flex;
  gap: 8px;
  padding-top: 12px;
  border-top: 1px solid var(--border-soft);
}
</style>
