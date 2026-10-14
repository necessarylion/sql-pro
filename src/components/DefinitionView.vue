<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, useTemplateRef, watch } from 'vue'
import { CopyIcon, LockIcon, PencilIcon } from 'lucide-vue-next'
import { OBJECT_KIND_LABELS } from '../constants'
import type { Session } from '../services/Session'
import { SqlEditor } from '../services/SqlEditor'
import BottomBar from './BottomBar.vue'

// Shows the CREATE statement of a view, trigger or routine. The text cannot be changed here.
const props = defineProps<{ session: Session }>()

const host = useTemplateRef<HTMLDivElement>('host')
const title = computed(describe)
// Not a ref: Vue must not wrap the CodeMirror objects.
let editor: SqlEditor | null = null

/** Like 'Trigger "audit" on orders'. */
function describe(): string {
  const object = props.session.definition?.object
  if (!object) {
    return ''
  }
  const name =
    object.schema && object.schema !== 'public' ? `${object.schema}.${object.name}` : object.name
  const table = object.table ? ` on ${object.table}` : ''
  return `${OBJECT_KIND_LABELS[object.kind]} "${name}"${table}`
}

function currentSql(): string {
  return props.session.definition?.sql ?? ''
}

function showSql(text: string): void {
  editor?.setText(text)
}

function createEditor(): void {
  if (host.value) {
    editor = new SqlEditor({ parent: host.value, text: currentSql(), readOnly: true })
    editor.setSchema(props.session.config?.driver, [])
  }
}

function destroyEditor(): void {
  editor?.destroy()
  editor = null
}

async function copySql(): Promise<void> {
  await navigator.clipboard.writeText(currentSql())
  props.session.status = 'Copied to the clipboard'
}

onMounted(createEditor)
onBeforeUnmount(destroyEditor)
watch(currentSql, showSql)
</script>

<template>
  <div class="bar head">
    <span class="title">{{ title }}</span>
    <span class="pill">
      <LockIcon :size="10" />
      Read only
    </span>
    <span class="spacer" />
    <button title="Copy the SQL" @click="session.run(copySql())">
      <CopyIcon :size="12" />
      Copy
    </button>
    <button title="Open the SQL in the Query view to change it" @click="session.editDefinition()">
      <PencilIcon :size="12" />
      Edit in Query
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
