<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, useTemplateRef, watch } from 'vue'
import { confirm } from '@tauri-apps/plugin-dialog'
import { LoaderCircleIcon, PlusIcon, SendIcon, SquareIcon, Trash2Icon } from 'lucide-vue-next'
import { AI_CUSTOM_MODEL, AI_MODES, AI_PROVIDERS } from '../constants/ai'
import { AiStore } from '../services/AiStore'
import { AI_STORE, CHAT_FORMATTER, injectRequired } from '../services/keys'
import type { Session } from '../services/Session'
import type { GridColumn, SavedChat } from '../types'
import BottomBar from './BottomBar.vue'
import DataGrid from './DataGrid.vue'
import PaneResizer from './PaneResizer.vue'

const props = defineProps<{ session: Session }>()
const store = injectRequired(AI_STORE)
const formatter = injectRequired(CHAT_FORMATTER)

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
  question.value = ''
  props.session.run(askAndSave(text))
}

/** Saves the chat also when the question fails, so the question is not lost. */
async function askAndSave(text: string): Promise<void> {
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

onMounted(observe)
onBeforeUnmount(unobserve)
// A new message, the busy line, its text ("Running SQL…" or "thinking") and a new result all move the end.
watch([entryCount, working, phase, hasResult], scrollDown)
</script>

<template>
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
      <div v-if="entry.kind === 'sql'" class="sql">
        <!-- ChatFormatter escapes all text. Only its own colour spans are HTML. -->
        <pre v-html="formatter.sql(entry.text, session.config?.driver)" />
        <span v-if="entry.outcome" class="outcome" :class="{ error: entry.failed }">
          {{ entry.outcome }}
        </span>
      </div>
      <!-- Markdown with raw HTML, links and images turned off. See ChatFormatter. -->
      <div
        v-else-if="entry.kind === 'ai'"
        class="entry ai markdown"
        v-html="formatter.markdown(entry.text, session.config?.driver)"
      />
      <div v-else :class="['entry', entry.kind, { failed: entry.failed }]">{{ entry.text }}</div>
    </template>
    <div v-if="working" class="entry note">
      <LoaderCircleIcon :size="12" class="spin" />
      {{ session.chat.phase === 'running' ? 'Running SQL…' : 'AI is thinking…' }}
    </div>
  </div>
  <div v-if="session.chat.result" class="result">
    <PaneResizer size="aiResultHeight" />
    <DataGrid v-model:selected="selected" :columns="columns" :rows="session.chat.result.rows" />
  </div>
  <div class="ask">
    <textarea
      v-model="question"
      rows="2"
      placeholder="Like: how many users signed up in the last 7 days?"
      @keydown="onKey"
    />
    <button v-if="working" title="Stop the AI" @click="stop">
      <SquareIcon :size="11" />
      Stop
    </button>
    <button v-else class="primary" title="Send (Enter)" :disabled="!question.trim()" @click="send">
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
  </BottomBar>
</template>

<style scoped>
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
  display: block;
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
