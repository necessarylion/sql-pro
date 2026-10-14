<script setup lang="ts">
import { computed, reactive, ref, useTemplateRef } from 'vue'
import {
  ChevronRightIcon,
  CircleAlertIcon,
  FolderPlusIcon,
  PlusIcon,
  SearchIcon,
  XIcon,
} from 'lucide-vue-next'
import { confirm, open } from '@tauri-apps/plugin-dialog'
import { DEFAULT_CONFIGS, DEFAULT_SSH_CONFIG, DRIVER_LABELS } from '../constants'
import {
  DEFAULT_ENVIRONMENT,
  ENVIRONMENT_HINT,
  ENVIRONMENT_KINDS,
  ENVIRONMENT_MAX_LENGTH,
} from '../constants/environment'
import type { EnvironmentKind } from '../constants/environment'
import { DRAG_START_DISTANCE } from '../constants/folders'
import {
  KEYCHAIN_DELETE_NOTE,
  KEYCHAIN_READ_NOTE,
  KEYCHAIN_WRITE_NOTE,
} from '../constants/keychain'
import { Environment } from '../services/Environment'
import type { Favorite, FavoriteSection, Folder } from '../services/FavoriteStore'
import { FAVORITE_STORE, injectRequired } from '../services/keys'
import type { Session } from '../services/Session'
import type { ConnectionConfig, Driver } from '../types'
import ContextMenu from './ContextMenu.vue'
import DbBadge from './DbBadge.vue'
import EnvBadge from './EnvBadge.vue'
import FolderDialog from './FolderDialog.vue'
import FolderIcon from './FolderIcon.vue'
import PaneResizer from './PaneResizer.vue'

const props = defineProps<{ session: Session }>()
const store = injectRequired(FAVORITE_STORE)
const folderDialog = useTemplateRef<InstanceType<typeof FolderDialog>>('folderDialog')

// `id` is the id of the loaded favorite. '' means a new connection.
// `folderId` is the folder of the connection. '' means no folder.
const form = reactive({ id: '', name: '', folderId: '', config: { ...DEFAULT_CONFIGS.mysql } })

/** The environment switch. `custom` is the typed name. It counts only for the kind `custom`. */
const env = reactive({ kind: DEFAULT_ENVIRONMENT as EnvironmentKind, custom: '' })

/**
 * A calm note about the keychain. It is not an error: the action itself worked.
 * `detail` is the raw keychain text. It is shown only as a tooltip.
 */
const note = reactive({ text: '', detail: '' })

const search = ref('')
const sections = computed(shownSections)

/** The right-click menu that is open now. It is for a folder or for a connection. */
const menu = ref<{ x: number; y: number; folder: Folder | null; favorite: Favorite | null } | null>(
  null,
)

/**
 * The connection row under the mouse button. `moved` turns true when the drag starts.
 * `over` is the folder id under the mouse. '' = the top level. null = no drop place.
 */
const drag = reactive({
  favorite: null as Favorite | null,
  moved: false,
  x: 0,
  y: 0,
  over: null as string | null,
})

function shownSections(): FavoriteSection[] {
  return store.sections(search.value)
}

/** A click opens or closes the folder. Not while searching: then every shown folder is open. */
function toggleFolder(folder: Folder): void {
  if (!search.value.trim()) {
    store.toggleFolder(folder.id)
  }
}

/** Opens the menu of a folder. */
function openFolderMenu(event: MouseEvent, folder: Folder): void {
  menu.value = { x: event.clientX, y: event.clientY, folder, favorite: null }
}

/** Opens the "Move to" menu of a connection. There is nothing to move to with no folders. */
function openFavoriteMenu(event: MouseEvent, favorite: Favorite): void {
  if (store.folders.length > 0) {
    menu.value = { x: event.clientX, y: event.clientY, folder: null, favorite }
  }
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
 * The mouse button goes down on a connection row. The row takes all moves from now on.
 * This uses pointer events and not HTML drag and drop: the Tauri window takes the
 * HTML drag events for file drops, so they do not reach the page on every platform.
 */
function startDrag(event: PointerEvent, favorite: Favorite): void {
  drag.moved = false
  // The delete button keeps its own click.
  if ((event.target as HTMLElement).closest('button')) {
    return
  }
  ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
  drag.favorite = favorite
  drag.x = event.clientX
  drag.y = event.clientY
}

/** Finds the drop place under the mouse. A small move is still a click. */
function moveDrag(event: PointerEvent): void {
  if (!drag.favorite) {
    return
  }
  const distance = Math.hypot(event.clientX - drag.x, event.clientY - drag.y)
  if (!drag.moved && distance < DRAG_START_DISTANCE) {
    return
  }
  drag.moved = true
  // ponytail: the list does not scroll by itself during a drag. The mouse wheel still works.
  const place = document
    .elementFromPoint(event.clientX, event.clientY)
    ?.closest<HTMLElement>('[data-drop]')
  drag.over = place ? (place.dataset.drop ?? '') : null
}

/** The mouse button goes up: the connection moves to the place under the mouse. */
function endDrag(): void {
  const favorite = drag.favorite
  const over = drag.over
  cancelDrag()
  if (favorite && over !== null) {
    moveTo(favorite, over)
  }
}

function cancelDrag(): void {
  drag.favorite = null
  drag.over = null
}

/** A click loads the connection. The click at the end of a drag does nothing. */
function clickRow(favorite: Favorite): void {
  if (!drag.moved) {
    void props.session.run(load(favorite))
  }
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
  form.id = ''
  form.name = ''
  form.folderId = ''
  setEnvironment(DEFAULT_ENVIRONMENT)
  form.config = { ...DEFAULT_CONFIGS.mysql }
}

function setDriver(driver: Driver): void {
  form.config = { ...DEFAULT_CONFIGS[driver] }
}

/** Turns the SSH fields on or off. */
function toggleSsh(): void {
  form.config.ssh = form.config.ssh ? null : { ...DEFAULT_SSH_CONFIG }
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

/** Returns false when the keychain did not give out a saved password. */
async function load(favorite: Favorite): Promise<boolean> {
  const loaded = await store.fullConfig(favorite)
  form.id = favorite.id
  form.name = favorite.name
  form.folderId = store.folderOf(favorite)
  setEnvironment(favorite.environment)
  form.config = loaded.config
  setNote(KEYCHAIN_READ_NOTE, loaded.keychainError)
  return !loaded.keychainError
}

async function connect(): Promise<void> {
  setNote('', '')
  await props.session.connect(currentConfig(), title(), environment())
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
}

/** Asks first. The saved password goes away with the connection. */
async function removeFavorite(favorite: Favorite): Promise<void> {
  const sure = await confirm(
    `Delete the saved connection "${favorite.name}"? Its saved password is removed too. This cannot be undone.`,
    { title: 'Delete connection', kind: 'warning' },
  )
  if (sure) {
    setNote(KEYCHAIN_DELETE_NOTE, await store.remove(favorite.id))
  }
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
      <!-- `data-drop` marks a drop place for a dragged connection. '' = the top level. -->
      <ul class="list" data-drop="">
        <template v-for="section in sections" :key="section.folder?.id ?? ''">
          <li
            v-if="section.folder"
            class="folder"
            :class="{ drop: drag.over === section.folder.id }"
            :data-drop="section.folder.id"
            :title="section.folder.name"
            @click="toggleFolder(section.folder)"
            @contextmenu.prevent="openFolderMenu($event, section.folder)"
          >
            <ChevronRightIcon class="chevron" :class="{ open: section.open }" :size="10" />
            <FolderIcon :icon="section.folder.icon" :color="section.folder.color" :size="13" />
            <span class="name">{{ section.folder.name }}</span>
            <span class="count">{{ section.favorites.length }}</span>
          </li>
          <template v-if="section.open">
            <li
              v-for="favorite in section.favorites"
              :key="favorite.id"
              :class="{
                selected: favorite.id === form.id,
                child: section.folder,
                dragged: drag.moved && drag.favorite === favorite,
              }"
              :data-drop="section.folder?.id ?? ''"
              @click="clickRow(favorite)"
              @dblclick="session.run(loadAndConnect(favorite))"
              @contextmenu.prevent="openFavoriteMenu($event, favorite)"
              @pointerdown.left="startDrag($event, favorite)"
              @pointermove="moveDrag"
              @pointerup="endDrag"
              @pointercancel="cancelDrag"
            >
              <DbBadge :driver="favorite.config.driver" />
              <span class="name">{{ favorite.name }}</span>
              <EnvBadge class="env" :name="favorite.environment" />
              <button
                type="button"
                class="icon remove"
                title="Delete connection"
                @click.stop="session.run(removeFavorite(favorite))"
              >
                <XIcon :size="11" />
              </button>
            </li>
          </template>
        </template>
      </ul>
      <!-- Shown while a connection of a folder is dragged. -->
      <div
        v-if="drag.moved && drag.favorite && store.folderOf(drag.favorite)"
        class="drop-top"
        :class="{ drop: drag.over === '' }"
        data-drop=""
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
          class="sidebar-footer new-folder"
          title="New folder"
          @click="folderDialog?.open(null)"
        >
          <FolderPlusIcon :size="13" />
        </button>
      </div>
      <PaneResizer size="sidebarWidth" />
    </aside>

    <div class="main">
      <form class="form" @submit.prevent="session.run(connect())">
        <header>
          <h1>New connection</h1>
          <p>Pick a database type, then fill in the details.</p>
        </header>

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

        <div v-if="session.error" class="banner error">
          <CircleAlertIcon :size="13" />
          <span>{{ session.error }}</span>
        </div>
        <div v-else-if="session.busy" class="banner">Working…</div>
        <div v-else-if="note.text" class="banner note" :title="note.detail">
          <CircleAlertIcon :size="13" />
          <span>{{ note.text }}</span>
        </div>

        <div class="actions">
          <span class="spacer" />
          <button type="button" :disabled="session.busy" @click="session.run(saveFavorite())">
            Save connection
          </button>
          <button type="submit" class="primary" :disabled="session.busy">
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

/* A connection inside a folder. */
.list li.child {
  padding-left: 26px;
}

.list li.dragged {
  opacity: 0.5;
}

.folder {
  font-weight: 600;
}

.chevron {
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

/* The last row of the sidebar: "New connection" and the small "New folder" button. */
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

.footer .new-folder {
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
  text-align: right;
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
