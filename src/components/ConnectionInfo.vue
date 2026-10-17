<script setup lang="ts">
import { computed } from 'vue'
import { DEFAULT_SSL_CONFIG, DRIVER_LABELS, SSL_MODE_LABELS } from '../constants'
import type { ConnectionConfig } from '../types'
import DbBadge from './DbBadge.vue'
import EnvBadge from './EnvBadge.vue'

/** One line of the view. `muted` marks a text that stands for "no value". */
interface InfoRow {
  label: string
  text: string
  mono: boolean
  muted: boolean
}

const EMPTY_TEXT = '—'
const PASSWORD_SET_TEXT = '••••••••'
const PASSWORD_EMPTY_TEXT = 'Not set'
const NO_FOLDER_TEXT = 'No folder'

/** A saved connection as plain text. `folderName` is '' for no folder. */
const props = defineProps<{
  name: string
  environment: string
  folderName: string
  config: ConnectionConfig
}>()

const rows = computed(buildRows)

function row(label: string, value: string, mono: boolean, empty = EMPTY_TEXT): InfoRow {
  return { label, text: value || empty, mono, muted: !value }
}

/** A password is never shown. The row only says if one is set. */
function secret(label: string, password: string): InfoRow {
  return {
    label,
    text: password ? PASSWORD_SET_TEXT : PASSWORD_EMPTY_TEXT,
    mono: false,
    muted: !password,
  }
}

function address(host: string, port: number): string {
  return host ? `${host}:${port}` : ''
}

function buildRows(): InfoRow[] {
  const config = props.config
  const list = [row('Folder', props.folderName, false, NO_FOLDER_TEXT)]
  if (config.driver === 'sqlite') {
    list.push(row('Database file', config.database, true))
    return list
  }
  list.push(
    row('Host', address(config.host, config.port), true),
    row('Username', config.user, true),
    secret('Password', config.password),
    row('Database', config.database, false),
  )
  const ssl = config.ssl ?? DEFAULT_SSL_CONFIG
  list.push(row('SSL', SSL_MODE_LABELS[ssl.mode], false))
  // Only the files that are set. Most connections have none.
  for (const [label, path] of [
    ['CA file', ssl.caPath],
    ['Client cert', ssl.certPath],
    ['Client key', ssl.keyPath],
  ]) {
    if (path) {
      list.push(row(label, path, true))
    }
  }
  if (config.ssh) {
    list.push(
      row('SSH Host', address(config.ssh.host, config.ssh.port), true),
      row('SSH User', config.ssh.user, true),
      secret('SSH Password', config.ssh.password),
      row('SSH Key', config.ssh.keyPath, true),
    )
  }
  return list
}
</script>

<template>
  <div class="info">
    <header>
      <h1 :title="name">{{ name }}</h1>
      <p>
        <DbBadge :driver="config.driver" />
        <span>{{ DRIVER_LABELS[config.driver] }}</span>
        <EnvBadge :name="environment" />
      </p>
    </header>

    <dl>
      <div v-for="item in rows" :key="item.label" class="row">
        <dt>{{ item.label }}</dt>
        <dd :class="{ mono: item.mono, muted: item.muted }" :title="item.muted ? '' : item.text">
          {{ item.text }}
        </dd>
      </div>
    </dl>
  </div>
</template>

<style scoped>
.info {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

/* The same title as the form has. */
h1 {
  margin: 0 0 6px;
  overflow: hidden;
  font-size: 17px;
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
  -webkit-user-select: text;
  user-select: text;
}

header p {
  display: flex;
  gap: 7px;
  align-items: center;
  margin: 0;
  color: var(--text-2);
}

dl {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin: 0;
}

/* The same label column as the rows of the form in ConnectionForm.vue. */
.row {
  display: flex;
  gap: 10px;
  align-items: center;
  height: 24px;
}

dt {
  flex: none;
  width: 92px;
  color: var(--text-2);
}

/* A long value, like a file path, is cut with "…". The tooltip has all of it. */
dd {
  flex: 1;
  min-width: 0;
  margin: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  -webkit-user-select: text;
  user-select: text;
}

dd.mono {
  font-family: var(--font-mono);
  font-size: 11px;
}

dd.muted {
  color: var(--text-3);
}
</style>
