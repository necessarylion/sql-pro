<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, useTemplateRef, watch } from 'vue'
import { confirm, open } from '@tauri-apps/plugin-dialog'
import {
  CheckIcon,
  FileTextIcon,
  LoaderCircleIcon,
  PaperclipIcon,
  PlusIcon,
  SendIcon,
  SquareIcon,
  TerminalIcon,
  Trash2Icon,
  UploadIcon,
  XIcon,
} from 'lucide-vue-next'
import {
  AI_CUSTOM_MODEL,
  AI_FILE_EXTENSIONS,
  AI_MODES,
  AI_PASTE_NAMES,
  AI_PRIVACY_NOTE,
  AI_PROVIDERS,
} from '../constants/ai'
import { AiAttachments } from '../services/AiAttachments'
import type { FileDropEvent } from '../services/FileDrop'
import { AiStore } from '../services/AiStore'
import { AI_STORE, CHAT_FORMATTER, FILE_DROP, injectRequired } from '../services/keys'
import type { Session } from '../services/Session'
import type { ChatEntry, GridColumn, SavedChat } from '../types'
import BottomBar from './BottomBar.vue'
import DataGrid from './DataGrid.vue'
import PaneResizer from './PaneResizer.vue'

const props = defineProps<{ session: Session }>()
const store = injectRequired(AI_STORE)
const formatter = injectRequired(CHAT_FORMATTER)
const fileDrop = injectRequired(FILE_DROP)
const root = useTemplateRef<HTMLDivElement>('root')
/** True while files are dragged over this view. It shows the drop overlay. */
const dropping = ref(false)

const question = ref('')
const selected = ref<number[]>([])
const log = useTemplateRef<HTMLDivElement>('log')
const columns = computed(resultColumns)
const working = computed(isWorking)
const connection = computed(connectionKey)
const history = computed(chatsOfConnection)
const saved = computed(isSaved)

function resultColumns(): GridColumn[] {
  const columns: GridColumn[] = []
  for (const name of props.session.chat.result?.columns ?? []) {
    columns.push({ name })
  }
  return columns
}

function isWorking(): boolean {
  return props.session.chat.phase !== ''
}

function connectionKey(): string {
  return AiStore.connectionKey(props.session.config)
}

function chatsOfConnection(): SavedChat[] {
  return store.chatsOf(connection.value)
}

/** True when the shown chat is in the history. */
function isSaved(): boolean {
  for (const chat of history.value) {
    if (chat.id === props.session.chat.chatId) {
      return true
    }
  }
  return false
}

function send(): void {
  const text = question.value.trim()
  if (!text || working.value) {
    return
  }
  props.session.run(askAndSave(text))
}

/**
 * Saves the chat also when the question fails, so the question is not lost.
 * A production connection asks once before its data goes to the AI.
 */
async function askAndSave(text: string): Promise<void> {
  const allowed = await store.allowSend(connection.value, props.session.environment, props.session)
  if (!allowed) {
    return
  }
  question.value = ''
  try {
    await props.session.askAi(text, store.options(chatProvider()))
  } finally {
    const chat = props.session.chat.snapshot(connection.value, props.session.config?.database ?? '')
    if (chat) {
      store.save(chat)
    }
  }
}

/** A new chat takes the chosen provider. An old chat keeps its own. */
function chatProvider(): string {
  return props.session.chat.entries.length > 0 ? props.session.chat.provider : store.provider
}

/** Enter sends. Shift+Enter makes a new line. */
function onKey(event: KeyboardEvent): void {
  if (event.key === 'Enter' && !event.shiftKey && !event.isComposing) {
    event.preventDefault()
    send()
  }
}

/** The paperclip button: pick images and CSV files. */
async function pickFiles(): Promise<void> {
  const picked = await open({
    multiple: true,
    filters: [{ name: 'Images and CSV files', extensions: AI_FILE_EXTENSIONS }],
  })
  await attachAll(picked ?? [])
}

/** Adds the files one by one. The first refused file stops it with its reason. */
async function attachAll(paths: string[]): Promise<void> {
  for (const path of paths) {
    await props.session.chat.attachFile(path)
  }
}

/** True when the page point lies on this view, and the view can be seen. */
function isOver(x: number, y: number): boolean {
  const box = root.value?.getBoundingClientRect()
  if (!box || box.width === 0 || box.height === 0) {
    return false
  }
  return x >= box.left && x <= box.right && y >= box.top && y <= box.bottom
}

/** Files dragged from the desktop. Only a drop on this view is taken. */
function onFileDrop(event: FileDropEvent): void {
  if (event.type === 'leave') {
    dropping.value = false
    return
  }
  const over = isOver(event.x, event.y)
  dropping.value = over && event.type !== 'drop'
  if (event.type === 'drop' && over && !working.value) {
    props.session.run(attachAll(event.paths))
  }
}

/** An image pasted into the input. Text pastes as usual. */
function onPaste(event: ClipboardEvent): void {
  for (const item of event.clipboardData?.items ?? []) {
    const name = AI_PASTE_NAMES[item.type]
    const file = item.kind === 'file' ? item.getAsFile() : null
    if (name && file) {
      event.preventDefault()
      props.session.run(attachPasted(name, file))
    }
  }
}

async function attachPasted(name: string, file: File): Promise<void> {
  const data = AiAttachments.toBase64(new Uint8Array(await file.arrayBuffer()))
  await props.session.chat.attachPasted(name, data)
}

/** Like "users.csv · 120 rows". An old chat did not keep the file. */
function fileLabel(file: { name: string; rows: number; mediaType: string }): string {
  return AiAttachments.isCsv(file) ? `${file.name} · ${file.rows} rows` : file.name
}

/** The approval card of a write: run the SQL as it is in the box now. */
function approve(entry: ChatEntry): void {
  props.session.chat.decide(entry.text)
}

function stop(): void {
  props.session.run(props.session.chat.stop(props.session.id))
}

function pickChat(event: Event): void {
  const id = (event.target as HTMLSelectElement).value
  for (const chat of history.value) {
    if (chat.id === id) {
      props.session.chat.restore(chat)
    }
  }
}

async function deleteChat(): Promise<void> {
  const ok = await confirm('The chat is deleted from the history.', {
    title: 'Delete chat',
    kind: 'warning',
  })
  if (ok) {
    store.remove(props.session.chat.chatId)
    props.session.chat.clear()
  }
}

/** Like "Count users · shop · 3 Oct". */
function chatLabel(chat: SavedChat): string {
  const day = new Date(chat.updated).toLocaleDateString(undefined, {
    day: 'numeric',
    month: 'short',
  })
  return [chat.title, chat.database, day].filter(Boolean).join(' · ')
}

function saveSettings(): void {
  store.saveSettings()
}

/** True while the user picked "Custom…" and has not saved an id yet. */
const customOpen = ref(false)
const modelChoice = computed({ get: readModelChoice, set: writeModelChoice })

/** A model that is not in the list is a custom one. */
function readModelChoice(): string {
  return customOpen.value || !store.knows(store.model) ? AI_CUSTOM_MODEL : store.model
}

function writeModelChoice(value: string): void {
  customOpen.value = value === AI_CUSTOM_MODEL
  if (!customOpen.value) {
    store.model = value
    store.saveSettings()
  }
}

/** A model id that does not have the allowed form goes back to the default model. */
function saveCustom(event: Event): void {
  store.model = (event.target as HTMLInputElement).value.trim()
  store.saveSettings()
}

function entryCount(): number {
  return props.session.chat.entries.length
}

function phase(): string {
  return props.session.chat.phase
}

function hasResult(): boolean {
  return props.session.chat.result !== null
}

/** True while the log shows its last line. A log the user scrolled up stays where it is. */
let pinned = true
let observer: ResizeObserver | null = null

function trackScroll(): void {
  const box = log.value
  if (box) {
    pinned = box.scrollTop + box.clientHeight >= box.scrollHeight - 4
  }
}

/** Keeps the newest message in sight. Runs after Vue has drawn the new line. */
async function scrollDown(): Promise<void> {
  await nextTick()
  log.value?.scrollTo({ top: log.value.scrollHeight })
  pinned = true
}

/** The log gets shorter when the result grid shows up or is dragged taller. Keep its end in sight. */
function onResize(): void {
  if (pinned) {
    log.value?.scrollTo({ top: log.value.scrollHeight })
  }
}

function observe(): void {
  if (log.value) {
    observer = new ResizeObserver(onResize)
    observer.observe(log.value)
  }
}

function unobserve(): void {
  observer?.disconnect()
  observer = null
}

let stopDrop: (() => void) | null = null

async function listenForDrops(): Promise<void> {
  stopDrop = await fileDrop.listen({ onFileDrop })
}

function stopListening(): void {
  stopDrop?.()
  stopDrop = null
}

onMounted(observe)
onMounted(listenForDrops)
onBeforeUnmount(unobserve)
onBeforeUnmount(stopListening)
// A new message, the busy line, its text ("Running SQL…" or "thinking") and a new result all move the end.
watch([entryCount, working, phase, hasResult], scrollDown)
</script>

<template>
  <div ref="root" class="ask-view">
    <div class="bar head">
      <select
        class="history"
        title="Open an earlier chat of this connection"
        :value="saved ? session.chat.chatId : ''"
        :disabled="working || history.length === 0"
        @change="pickChat"
      >
        <option value="" disabled>{{ history.length ? 'History…' : 'No history' }}</option>
        <option v-for="chat in history" :key="chat.id" :value="chat.id">
          {{ chatLabel(chat) }}
        </option>
      </select>
      <button
        class="icon"
        title="Delete this chat from the history"
        :disabled="working || !saved"
        @click="session.run(deleteChat())"
      >
        <Trash2Icon :size="12" />
      </button>
      <span class="spacer" />
      <button
        title="Start a new chat. The AI forgets this one."
        :disabled="working || session.chat.entries.length === 0"
        @click="session.chat.clear()"
      >
        <PlusIcon :size="11" />
        New chat
      </button>
    </div>
    <div ref="log" class="log" @scroll="trackScroll">
      <div v-if="session.chat.entries.length === 0" class="empty">
        Ask about this database in plain English. The AI writes the SQL. Changes ask you first.
      </div>
      <template v-for="(entry, index) in session.chat.entries" :key="index">
        <!-- A write waits here. The user may change it, then Run asks once more in a dialog. -->
        <div v-if="entry.kind === 'sql' && entry.pending" class="sql card">
          <span class="card-title"
            >The AI wants to change data. Check the SQL, change it if needed.</span
          >
          <textarea v-model="entry.text" class="mono" rows="4" spellcheck="false" />
          <div class="card-buttons">
            <button class="primary" :disabled="!entry.text.trim()" @click="approve(entry)">
              <CheckIcon :size="11" />
              Run
            </button>
            <button @click="session.chat.decide(null)">Reject</button>
            <span class="spacer" />
            <button
              title="Put this SQL in the Query editor"
              @click="session.openInQuery(entry.text)"
            >
              <TerminalIcon :size="11" />
              Open in Query
            </button>
          </div>
        </div>
        <div v-else-if="entry.kind === 'sql'" class="sql">
          <!-- ChatFormatter escapes all text. Only its own colour spans are HTML. -->
          <pre v-html="formatter.sql(entry.text, session.config?.driver)" />
          <div class="outcome" :class="{ error: entry.failed }">
            <span class="outcome-text">{{ entry.outcome }}</span>
            <button
              class="icon"
              title="Open in Query: put this SQL in the Query editor"
              @click="session.openInQuery(entry.text)"
            >
              <TerminalIcon :size="11" />
            </button>
          </div>
        </div>
        <!-- Markdown with raw HTML, links and images turned off. See ChatFormatter. -->
        <div
          v-else-if="entry.kind === 'ai'"
          class="entry ai markdown"
          v-html="formatter.markdown(entry.text, session.config?.driver)"
        />
        <div v-else :class="['entry', entry.kind, { failed: entry.failed }]">
          {{ entry.text }}
          <span v-if="entry.files" class="files">
            <span v-for="file in entry.files" :key="file.name" class="chip" :title="file.name">
              <FileTextIcon :size="10" />
              <span class="chip-name">
                {{ fileLabel(file) }}{{ file.kept ? '' : ' (not saved)' }}
              </span>
            </span>
          </span>
        </div>
      </template>
      <div v-if="working" class="entry note">
        <LoaderCircleIcon :size="12" class="spin" />
        {{
          session.chat.phase === 'running'
            ? 'Running SQL…'
            : session.chat.phase === 'waiting'
              ? 'Waiting for you to approve the SQL…'
              : 'AI is thinking…'
        }}
      </div>
    </div>
    <div v-if="session.chat.result" class="result">
      <PaneResizer size="aiResultHeight" />
      <DataGrid v-model:selected="selected" :columns="columns" :rows="session.chat.result.rows" />
    </div>
    <div v-if="session.chat.attachments.items.length" class="chips">
      <span
        v-for="(file, index) in session.chat.attachments.items"
        :key="index"
        class="chip"
        :title="fileLabel(file)"
      >
        <img v-if="!AiAttachments.isCsv(file)" :src="AiAttachments.preview(file)" alt="" />
        <FileTextIcon v-else :size="11" />
        <span class="chip-name">{{ fileLabel(file) }}</span>
        <button
          class="icon chip-remove"
          :title="`Remove ${file.name}`"
          @click="session.chat.attachments.remove(index)"
        >
          <XIcon :size="11" />
        </button>
      </span>
    </div>
    <div class="ask">
      <button
        class="icon attach"
        title="Attach images (PNG, JPEG, WebP) or CSV files. You can also drop or paste them."
        :disabled="working"
        @click="session.run(pickFiles())"
      >
        <PaperclipIcon :size="13" />
      </button>
      <textarea
        v-model="question"
        rows="2"
        placeholder="Like: how many users signed up in the last 7 days?"
        @keydown="onKey"
        @paste="onPaste"
      />
      <button v-if="working" title="Stop the AI" @click="stop">
        <SquareIcon :size="11" />
        Stop
      </button>
      <button
        v-else
        class="primary"
        title="Send (Enter)"
        :disabled="!question.trim()"
        @click="send"
      >
        <SendIcon :size="11" />
        Ask
      </button>
    </div>
    <BottomBar :session="session">
      <select v-model="store.provider" title="AI provider" @change="saveSettings">
        <option v-for="(provider, id) in AI_PROVIDERS" :key="id" :value="id">
          {{ provider.label }}
        </option>
      </select>
      <select v-model="modelChoice" title="Model">
        <option v-for="choice in store.current.models" :key="choice.value" :value="choice.value">
          {{ choice.label }}
        </option>
        <option :value="AI_CUSTOM_MODEL">Custom…</option>
      </select>
      <input
        v-if="modelChoice === AI_CUSTOM_MODEL"
        class="custom mono"
        :value="store.model"
        placeholder="model id, like claude-opus-5-5"
        title="Any model id the AI tool knows. Lower case letters, digits, '.', '-', '[' and ']'."
        pattern="[a-z0-9][a-z0-9.\-\[\]]{0,79}"
        autocapitalize="off"
        spellcheck="false"
        @change="saveCustom"
      />
      <select v-model="store.effort" title="Effort" @change="saveSettings">
        <option v-for="choice in store.current.efforts" :key="choice.value" :value="choice.value">
          {{ choice.label }}
        </option>
      </select>
      <select
        v-model="store.mode"
        title="What happens with SQL that changes data"
        @change="saveSettings"
      >
        <option v-for="mode in AI_MODES" :key="mode.value" :value="mode.value">
          {{ mode.label }}
        </option>
      </select>
      <span class="divider" />
      <span class="dim privacy" :title="AI_PRIVACY_NOTE">{{ AI_PRIVACY_NOTE }}</span>
    </BottomBar>
    <div v-if="dropping" class="drop">
      <UploadIcon :size="22" />
      Drop images or CSV files to attach them
    </div>
  </div>
</template>

<style scoped>
/* The whole view. The drop overlay covers it. */
.ask-view {
  position: relative;
  display: flex;
  flex: 1;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
}

.drop {
  position: absolute;
  inset: 6px;
  z-index: 20;
  display: flex;
  gap: 8px;
  align-items: center;
  justify-content: center;
  border: 2px dashed var(--accent);
  border-radius: 8px;
  background: color-mix(in srgb, var(--bg) 85%, transparent);
  color: var(--accent-text);
  font-size: 13px;
  font-weight: 600;
  pointer-events: none;
}

.chips,
.files {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}

.chips {
  flex: none;
  /* The same space above and below. The input row below has its own border line. */
  padding: 6px 8px;
  background: var(--toolbar);
  border-top: 1px solid var(--border);
}

.files {
  margin-top: 4px;
}

/* The chips start where the input box starts: after the paperclip (22px) and the gap (6px). */
.chips {
  padding-left: 36px;
}

/* A small control, like the others in the bars. */
.chip {
  display: inline-flex;
  flex: none;
  gap: 4px;
  align-items: center;
  max-width: 240px;
  height: 22px;
  padding: 0 2px 0 5px;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 5px;
  color: var(--text);
  font-size: 11px;
  font-weight: 400;
  white-space: nowrap;
}

.chip > :not(.chip-name) {
  flex-shrink: 0;
}

/* A long name is cut. The full name is in the title. */
.chip-name {
  min-width: 0;
  /* As tall as the thumbnail, so all three parts share one center line. */
  line-height: 16px;
  overflow: hidden;
  text-overflow: ellipsis;
}

.chip img {
  width: 16px;
  height: 16px;
  object-fit: cover;
  border-radius: 3px;
}

.chip-remove.icon {
  width: 16px;
  height: 16px;
  border-radius: 3px;
  color: var(--text-2);
}

.chip-remove.icon:hover {
  background: var(--seg-active);
  color: var(--text);
}

/* User messages are on the accent colour. Their chips keep the normal colours. */
.user .chip {
  color: var(--text);
}

.attach {
  align-self: center;
}

/* The approval card of a write. */
.card {
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-width: 320px;
  padding: 6px 8px;
  border-color: var(--warn);
}

.card-title {
  color: var(--text-2);
  font-size: 11px;
}

.card textarea {
  padding: 4px 6px;
  border-radius: 5px;
  font-family: var(--font-mono);
  font-size: 11px;
  resize: vertical;
}

.card-buttons {
  display: flex;
  gap: 6px;
}

.card-buttons button {
  height: 22px;
  padding: 0 10px;
  border-radius: 5px;
}

.privacy {
  overflow: hidden;
  text-overflow: ellipsis;
}

.outcome-text {
  flex: 1;
}

.log {
  display: flex;
  flex: 1 1 0;
  flex-direction: column;
  gap: 6px;
  /* Never squeezed to nothing by the result grid. It scrolls inside this height. */
  min-height: 120px;
  padding: 10px;
  overflow-y: auto;
  -webkit-user-select: text;
  user-select: text;
}

.entry {
  max-width: 80%;
  padding: 5px 9px;
  border-radius: 6px;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.user {
  align-self: flex-end;
  background: var(--accent);
  color: var(--on-accent);
}

.ai {
  align-self: flex-start;
  background: var(--toolbar);
  border: 1px solid var(--border-soft);
}

.note.failed {
  color: var(--danger);
  white-space: pre-wrap;
  -webkit-user-select: text;
  user-select: text;
}

.note {
  display: flex;
  gap: 6px;
  align-items: center;
  align-self: flex-start;
  color: var(--text-3);
}

.sql {
  align-self: flex-start;
  max-width: 80%;
  border: 1px solid var(--border-soft);
  border-radius: 6px;
  overflow: hidden;
}

/* The colours of SQL, the same as in the Query editor. See ChatFormatter. */
.log :deep(.sql-keyword) {
  color: var(--accent-text);
}

.log :deep(.sql-string) {
  color: var(--warn);
}

.log :deep(.sql-type) {
  color: var(--syntax-type);
}

.log :deep(.sql-comment) {
  color: var(--text-3);
  font-style: italic;
}

/* Markdown in AI replies. Compact, with the design tokens. */
.markdown {
  white-space: normal;
}

.markdown :deep(> :first-child) {
  margin-top: 0;
}

.markdown :deep(> :last-child) {
  margin-bottom: 0;
}

.markdown :deep(p),
.markdown :deep(ul),
.markdown :deep(ol),
.markdown :deep(pre),
.markdown :deep(table) {
  margin: 4px 0;
}

.markdown :deep(ul),
.markdown :deep(ol) {
  padding-left: 18px;
}

.markdown :deep(h1),
.markdown :deep(h2),
.markdown :deep(h3),
.markdown :deep(h4),
.markdown :deep(h5),
.markdown :deep(h6) {
  margin: 6px 0 2px;
  font-size: 12px;
  font-weight: 600;
}

.markdown :deep(code) {
  padding: 0 3px;
  background: var(--row-alt);
  border-radius: 3px;
  font-family: var(--font-mono);
  font-size: 11px;
}

.markdown :deep(pre) {
  padding: 5px 8px;
  background: var(--row-alt);
  border: 1px solid var(--border-soft);
  border-radius: 5px;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.markdown :deep(pre code) {
  padding: 0;
  background: none;
}

.markdown :deep(table) {
  border-collapse: collapse;
  font-size: 11px;
}

.markdown :deep(th),
.markdown :deep(td) {
  padding: 2px 6px;
  border: 1px solid var(--border);
  text-align: left;
}

.markdown :deep(th) {
  background: var(--header);
  font-weight: 600;
}

.markdown :deep(blockquote) {
  margin: 4px 0;
  padding-left: 8px;
  border-left: 2px solid var(--border);
  color: var(--text-2);
}

.sql pre {
  margin: 0;
  padding: 5px 9px;
  background: var(--row-alt);
  font-family: var(--font-mono);
  font-size: 11px;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.outcome {
  display: flex;
  align-items: center;
  padding: 2px 9px;
  border-top: 1px solid var(--border-soft);
  color: var(--text-2);
  font-size: 11px;
}

.outcome.error {
  color: var(--danger);
}

/* The log scrolls. Its messages keep their full height. */
.log > * {
  flex-shrink: 0;
}

/* The last result. It takes a part of the height, so the chat stays in sight. */
/* The grid has the dragged height. It gets smaller when the window is too small: the log keeps its 120px. */
.result {
  position: relative;
  display: flex;
  flex: 0 1 var(--ai-result-height);
  flex-direction: column;
  min-height: 80px;
  border-top: 1px solid var(--border);
}

.ask {
  display: flex;
  flex: none;
  gap: 6px;
  align-items: flex-end;
  padding: 6px 8px;
  background: var(--toolbar);
  border-top: 1px solid var(--border);
}

/* The same compact controls as the other bottom bars. */
.bar select {
  height: 22px;
  padding: 0 4px;
  border-radius: 5px;
  font-size: 11px;
}

.custom {
  width: 170px;
}

.custom:invalid {
  border-color: var(--danger);
}

.history {
  min-width: 0;
  max-width: 320px;
}

.ask textarea {
  flex: 1;
  min-height: 26px;
  max-height: 120px;
  padding: 4px 8px;
  border-radius: 5px;
  resize: vertical;
}
</style>
