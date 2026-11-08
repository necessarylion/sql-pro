<script setup lang="ts">
import { reactive, ref, useTemplateRef } from 'vue'
import { confirm, save } from '@tauri-apps/plugin-dialog'
import { PG_INSTALL_GUIDE, injectRequired } from '../services/keys'
import type { GuideLink, GuideStep } from '../services/PgInstallGuide'
import { PgToolLoader, type PgToolChoice } from '../services/PgToolChoice'
import type { Session } from '../services/Session'

// The PostgreSQL backup and restore dialog: the format of a backup, the pg_dump or
// pg_restore version, and for a restore whether to drop the objects first.
// A version that is not installed shows how to install it. The app installs nothing.
const props = defineProps<{ session: Session }>()
const guide = injectRequired(PG_INSTALL_GUIDE)
const dialog = useTemplateRef<HTMLDialogElement>('dialog')
const form = reactive({ restoring: false, path: '', custom: false, clean: false, problem: '' })
const choice = ref<PgToolChoice | null>(null)
const loader = new PgToolLoader()
const checking = ref(false)

function openBackup(): Promise<void> {
  return show(false, '')
}

/** `path` is the .dump file to load. */
function openRestore(path: string): Promise<void> {
  return show(true, path)
}

async function show(restoring: boolean, path: string): Promise<void> {
  form.restoring = restoring
  form.path = path
  form.clean = false
  choice.value = null
  dialog.value?.showModal()
  await load()
}

/** Loads the tool list. Keeps the picked version when it is still listed, so "Check again" stays on it. */
async function load(): Promise<void> {
  const picked = choice.value?.version ?? null
  form.problem = ''
  checking.value = true
  try {
    const next = await loader.load(props.session, form.restoring)
    if (next === null) {
      return
    }
    if (picked !== null) {
      next.pick(picked)
    }
    choice.value = next
    checking.value = false
  } catch (error) {
    form.problem = String(error)
    checking.value = false
  }
}

/** The install guide for the picked version. Empty when it is installed. */
function steps(): GuideStep[] {
  const current = choice.value
  return current === null || current.installed ? [] : guide.steps(current.version)
}

async function copy(command: string): Promise<void> {
  await navigator.clipboard.writeText(command)
  props.session.status = 'Copied to the clipboard'
}

async function openLink(link: GuideLink): Promise<void> {
  try {
    await guide.open(link)
  } catch (error) {
    form.problem = String(error)
  }
}

function close(): void {
  loader.drop()
  dialog.value?.close()
}

/** The dialog has no padding. A click on the dialog itself is a click outside the panel. */
function closeOnOutsideClick(event: MouseEvent): void {
  if (event.target === dialog.value) {
    close()
  }
}

/** True when the dialog may go on: the picked version is installed. */
function ready(): boolean {
  return choice.value !== null && choice.value.installed
}

async function submit(): Promise<void> {
  const version = choice.value?.version ?? null
  close()
  if (form.restoring) {
    await restore(version)
  } else {
    await backup(version)
  }
}

/** Asks where to save, then asks to confirm. The file type follows the format. Does nothing on cancel or no. */
async function backup(version: number | null): Promise<void> {
  const session = props.session
  const extension = form.custom ? 'dump' : 'sql'
  const path = await save({
    title: 'Back up database',
    defaultPath: session.backupName(new Date(), extension),
    filters: [{ name: form.custom ? 'pg_restore file' : 'SQL', extensions: [extension] }],
  })
  if (!path) {
    return
  }
  const sure = await confirm(
    `Back up database "${session.config?.database ?? ''}" to ${path}? A large database can take a few minutes.`,
    { title: 'Back up database', kind: 'info' },
  )
  if (sure) {
    await session.run(session.backup(path, { custom: form.custom, version }))
  }
}

/** Asks to confirm, naming the database. Restores only on yes. */
async function restore(version: number | null): Promise<void> {
  const session = props.session
  const database = session.config?.database ?? ''
  const drop = form.clean ? ' It first drops each object that is in the file.' : ''
  const sure = await confirm(
    `Restore ${form.path} into database "${database}"?${drop} This can change or delete data.`,
    { title: 'Restore from .dump', kind: 'warning' },
  )
  if (sure) {
    await session.run(session.restore(form.path, { version, clean: form.clean }))
  }
}

defineExpose({ openBackup, openRestore })
</script>

<template>
  <dialog ref="dialog" class="pg-tool-dialog" @click="closeOnOutsideClick">
    <form class="panel" @submit.prevent="submit">
      <h2>{{ form.restoring ? 'Restore from .dump' : 'Back up database' }}</h2>
      <p v-if="form.restoring" class="note file">{{ form.path }}</p>
      <div v-else class="row">
        <label for="pg-format">Format</label>
        <select id="pg-format" v-model="form.custom">
          <option :value="false">Plain SQL (.sql)</option>
          <option :value="true">Custom (.dump, for pg_restore)</option>
        </select>
      </div>
      <div class="row">
        <label for="pg-version">Tool</label>
        <select v-if="choice" id="pg-version" v-model="choice.version">
          <option v-for="option in choice.options" :key="option.version" :value="option.version">
            {{ choice.label(option) }}
          </option>
        </select>
        <select v-else id="pg-version" disabled>
          <option>Looking for tools…</option>
        </select>
      </div>
      <label v-if="form.restoring" class="check">
        <input v-model="form.clean" type="checkbox" />
        Drop objects first (--clean --if-exists)
      </label>
      <p v-if="choice?.warning" class="warning">{{ choice.warning }}</p>
      <p v-if="choice?.missing" class="error">{{ choice.missing }}</p>
      <div v-if="steps().length > 0" class="guide">
        <p class="note">
          {{ form.restoring ? 'pg_restore' : 'pg_dump' }} {{ choice?.version }} is not installed.
        </p>
        <div v-for="step in steps()" :key="step.text">
          <p class="note">
            {{ step.text }}
            <a
              v-for="link in step.links"
              :key="link.url"
              href="#"
              @click.prevent="openLink(link)"
              >{{ link.label }}</a
            >
          </p>
          <div v-if="step.command" class="command">
            <code>{{ step.command }}</code>
            <button type="button" @click="copy(step.command)">Copy</button>
          </div>
        </div>
      </div>
      <p v-if="form.problem" class="error">{{ form.problem }}</p>
      <div class="actions">
        <button type="button" class="check-again" :disabled="checking" @click="load">
          Check again
        </button>
        <button type="button" @click="close">Cancel</button>
        <button type="submit" class="primary" :disabled="!ready()">
          {{ form.restoring ? 'Restore…' : 'Back up…' }}
        </button>
      </div>
    </form>
  </dialog>
</template>

<style scoped>
.pg-tool-dialog {
  width: 340px;
  padding: 0;
  background: var(--toolbar);
  border: 1px solid var(--border);
  border-radius: 10px;
  box-shadow: 0 12px 32px #00000040;
  color: var(--text);
}

.pg-tool-dialog::backdrop {
  background: #00000040;
}

.panel {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 14px 16px 16px;
}

h2 {
  margin: 0;
  font-size: calc(13px * var(--font-scale));
  font-weight: 600;
}

.row {
  display: flex;
  gap: 10px;
  align-items: center;
}

.row label {
  flex: none;
  width: 50px;
  color: var(--text-2);
  line-height: 26px;
}

.row select {
  flex: 1;
  min-width: 0;
}

.check {
  display: flex;
  gap: 6px;
  align-items: center;
  font-size: calc(12px * var(--font-scale));
}

.check input {
  width: 13px;
  height: 13px;
  margin: 0;
  accent-color: var(--accent);
}

.note,
.warning,
.error {
  margin: 0;
  font-size: calc(11px * var(--font-scale));
  overflow-wrap: anywhere;
}

.note {
  color: var(--text-2);
}

.warning {
  color: var(--warn);
}

.guide {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.note a {
  margin-left: 6px;
  color: var(--accent-text);
}

.command {
  display: flex;
  gap: 6px;
  align-items: center;
}

.command code {
  flex: 1;
  min-width: 0;
  padding: 3px 6px;
  font-size: calc(11px * var(--font-scale));
  background: var(--bg);
  border: 1px solid var(--border-soft);
  border-radius: 4px;
  overflow-wrap: anywhere;
  user-select: text;
}

.check-again {
  margin-right: auto;
}

.actions {
  display: flex;
  gap: 8px;
  justify-content: flex-end;
  padding-top: 4px;
}
</style>
