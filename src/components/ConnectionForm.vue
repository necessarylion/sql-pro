<script setup lang="ts">
import { computed, reactive, ref } from 'vue'
import { CircleAlertIcon, PlusIcon, SearchIcon, XIcon } from 'lucide-vue-next'
import { open } from '@tauri-apps/plugin-dialog'
import { DEFAULT_CONFIGS, DEFAULT_SSH_CONFIG, DRIVER_LABELS } from '../constants'
import type { Favorite } from '../services/FavoriteStore'
import { FAVORITE_STORE, injectRequired } from '../services/keys'
import type { Session } from '../services/Session'
import type { ConnectionConfig, Driver } from '../types'
import DbBadge from './DbBadge.vue'

const props = defineProps<{ session: Session }>()
const store = injectRequired(FAVORITE_STORE)

// `id` is the id of the loaded favorite. '' means a new connection.
const form = reactive({ id: '', name: '', config: { ...DEFAULT_CONFIGS.mysql } })

const search = ref('')
const shownFavorites = computed(matchingFavorites)

function matchingFavorites(): Favorite[] {
  const needle = search.value.trim().toLowerCase()
  const matches: Favorite[] = []
  for (const favorite of store.favorites) {
    if (favorite.name.toLowerCase().includes(needle)) {
      matches.push(favorite)
    }
  }
  return matches
}

function reset(): void {
  form.id = ''
  form.name = ''
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

async function load(favorite: Favorite): Promise<void> {
  const config = await store.fullConfig(favorite)
  form.id = favorite.id
  form.name = favorite.name
  form.config = config
}

async function connect(): Promise<void> {
  await props.session.connect(currentConfig(), title())
}

async function loadAndConnect(favorite: Favorite): Promise<void> {
  await load(favorite)
  await connect()
}

async function saveFavorite(): Promise<void> {
  form.id = form.id || crypto.randomUUID()
  form.name = title()
  await store.save({ id: form.id, name: form.name, config: currentConfig() })
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
        <input v-model="search" placeholder="Filter favorites" spellcheck="false" />
      </div>
      <ul class="list">
        <li
          v-for="favorite in shownFavorites"
          :key="favorite.id"
          :class="{ selected: favorite.id === form.id }"
          @click="session.run(load(favorite))"
          @dblclick="session.run(loadAndConnect(favorite))"
        >
          <DbBadge :driver="favorite.config.driver" />
          <span class="name">{{ favorite.name }}</span>
          <button
            type="button"
            class="icon remove"
            title="Delete favorite"
            @click.stop="session.run(store.remove(favorite.id))"
          >
            <XIcon :size="11" />
          </button>
        </li>
      </ul>
      <button type="button" class="sidebar-footer" @click="reset">
        <PlusIcon :size="12" />
        New favorite
      </button>
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

        <div class="actions">
          <span class="spacer" />
          <button type="button" :disabled="session.busy" @click="session.run(saveFavorite())">
            Save as favorite
          </button>
          <button type="submit" class="primary" :disabled="session.busy">
            {{ form.config.driver === 'sqlite' ? 'Open' : 'Connect' }}
            <span class="hint">↵</span>
          </button>
        </div>
      </form>
    </div>
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
  flex-direction: column;
  width: 240px;
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

.row input {
  flex: 1;
  min-width: 0;
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
