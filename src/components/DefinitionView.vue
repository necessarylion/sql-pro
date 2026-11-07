<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, useTemplateRef, watch } from 'vue'
import { CopyIcon } from 'lucide-vue-next'
import { OBJECT_KIND_LABELS } from '../constants'
import { SHORTCUTS, injectRequired } from '../services/keys'
import type { Session } from '../services/Session'
import { SchemaName } from '../services/SchemaName'
import { SqlEditor } from '../services/SqlEditor'
import BottomBar from './BottomBar.vue'

// Shows and edits the CREATE statement of a view, trigger or routine, or of a new one.
const props = defineProps<{ session: Session }>()
const shortcuts = injectRequired(SHORTCUTS)

const host = useTemplateRef<HTMLDivElement>('host')
const title = computed(describe)
const isNew = computed(readIsNew)
// Not a ref: Vue must not wrap the CodeMirror objects.
let editor: SqlEditor | null = null

/** Like 'Trigger "audit" on orders', or 'New trigger'. */
function describe(): string {
  const shown = props.session.definition
  if (!shown) {
    return ''
  }
  const kind = OBJECT_KIND_LABELS[shown.kind]
  const object = shown.object
  if (!object) {
    return `New ${kind.toLowerCase()}`
  }
  const name = SchemaName.label(object.schema, object.name)
  const table = object.table ? ` on ${object.table}` : ''
  return `${kind} "${name}"${table}`
}

function readIsNew(): boolean {
  return props.session.definition?.object === null
}

function currentText(): string {
  return props.session.definitionText
}

function showText(text: string): void {
  editor?.setText(text)
}

function keepText(text: string): void {
  props.session.definitionText = text
}

function createEditor(): void {
  if (host.value) {
    editor = new SqlEditor({ parent: host.value, text: currentText(), onChange: keepText })
    editor.setSchema(props.session.config?.driver, props.session.tables)
  }
}

function destroyEditor(): void {
  editor?.destroy()
  editor = null
}

async function copySql(): Promise<void> {
  await navigator.clipboard.writeText(currentText())
  props.session.status = 'Copied to the clipboard'
}

function discard(): void {
  if (props.session.definition) {
    props.session.definitionText = props.session.definition.sql
  }
}

onMounted(createEditor)
onBeforeUnmount(destroyEditor)
watch(currentText, showText)
</script>

<template>
  <div class="bar head">
    <span class="title">{{ title }}</span>
    <span v-if="session.definitionChanged" class="pill unsaved"><span class="dot" />Unsaved</span>
    <span class="spacer" />
    <button title="Copy the SQL" @click="session.run(copySql())">
      <CopyIcon :size="12" />
      Copy
    </button>
    <button v-if="session.definitionChanged" :disabled="session.busy" @click="discard">
      Discard
    </button>
    <button
      class="primary"
      :disabled="session.busy || (!isNew && !session.definitionChanged)"
      @click="session.run(session.saveDefinition())"
    >
      {{ isNew ? 'Create' : 'Save' }}
      <span class="hint">{{ shortcuts.hint('S') }}</span>
    </button>
  </div>
  <div ref="host" class="host" />
  <BottomBar :session="session" />
</template>

<style scoped>
.head {
  gap: 8px;
}

.title {
  overflow: hidden;
  font-weight: 500;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.host {
  flex: 1;
  min-height: 0;
  overflow: hidden;
}
</style>
