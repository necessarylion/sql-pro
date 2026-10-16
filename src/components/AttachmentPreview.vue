<script setup lang="ts">
import { computed, ref, shallowRef, useTemplateRef } from 'vue'
import { XIcon } from 'lucide-vue-next'
import { AiAttachments } from '../services/AiAttachments'
import type { GridColumn, Row } from '../types'
import DataGrid from './DataGrid.vue'

/** The file to show: its name, type and data (base64 for an image). */
interface PreviewFile {
  name: string
  mediaType: string
  data: string
}

const dialog = useTemplateRef<HTMLDialogElement>('dialog')
const file = ref<PreviewFile | null>(null)
/** The CSV rows as a grid. Empty for an image. */
const columns = shallowRef<GridColumn[]>([])
const rows = shallowRef<Row[]>([])
const selected = ref<number[]>([])
const csv = computed(isCsv)
const caption = computed(captionText)

function isCsv(): boolean {
  return file.value !== null && AiAttachments.isCsv(file.value)
}

/** Like "photo.png · 240 KB" or "users.csv · 120 rows". Base64 holds 3 bytes in 4 characters. */
function captionText(): string {
  if (!file.value) {
    return ''
  }
  if (csv.value) {
    return `${file.value.name} · ${rows.value.length} rows`
  }
  const bytes = Math.round((file.value.data.length * 3) / 4)
  const size =
    bytes >= 1024 * 1024
      ? `${(bytes / 1024 / 1024).toFixed(1)} MB`
      : `${Math.ceil(bytes / 1024)} KB`
  return `${file.value.name} · ${size}`
}

/** Shows an image. The native dialog closes on Escape by itself. */
function open(image: PreviewFile): void {
  file.value = image
  dialog.value?.showModal()
}

/** Shows a CSV file. `records` are its rows, the first one is the header. */
function openCsv(text: PreviewFile, records: string[][]): void {
  const table = AiAttachments.csvTable(records)
  const gridColumns: GridColumn[] = []
  for (const name of table.columns) {
    gridColumns.push({ name })
  }
  columns.value = gridColumns
  rows.value = table.rows
  selected.value = []
  open(text)
}

function close(): void {
  dialog.value?.close()
}

/** A click on the dark area around the file closes it. */
function closeOnOutsideClick(event: MouseEvent): void {
  const target = event.target as HTMLElement
  if (target === dialog.value || target.classList.contains('frame')) {
    close()
  }
}

function forget(): void {
  file.value = null
  columns.value = []
  rows.value = []
}

defineExpose({ open, openCsv })
</script>

<template>
  <!-- `.stop` keeps Escape from the app's own keys, so it does not also stop a query. -->
  <dialog
    ref="dialog"
    class="preview"
    @click="closeOnOutsideClick"
    @keydown.esc.stop
    @close="forget"
  >
    <div v-if="file" class="frame" :class="{ csv }">
      <div v-if="csv" class="table">
        <DataGrid v-model:selected="selected" :columns="columns" :rows="rows" />
      </div>
      <img v-else :src="AiAttachments.preview(file)" :alt="file.name" />
      <div class="caption">
        <span class="name" :title="caption">{{ caption }}</span>
        <button class="icon" title="Close (Esc)" @click="close">
          <XIcon :size="13" />
        </button>
      </div>
    </div>
  </dialog>
</template>

<style scoped>
.preview {
  max-width: 100vw;
  max-height: 100vh;
  padding: 0;
  background: none;
  border: 0;
  color: var(--text);
}

.preview::backdrop {
  background: #000000c0;
}

.frame {
  display: flex;
  flex-direction: column;
  gap: 6px;
  align-items: center;
}

img {
  max-width: 90vw;
  max-height: calc(90vh - 34px);
  object-fit: contain;
  border-radius: 6px;
  box-shadow: var(--popover-shadow);
}

/* A CSV file: a large grid that scrolls inside. */
.table {
  display: flex;
  flex-direction: column;
  width: 90vw;
  height: 80vh;
  overflow: hidden;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 6px;
  box-shadow: var(--popover-shadow);
}

.caption {
  display: flex;
  gap: 6px;
  align-items: center;
  max-width: 90vw;
  padding: 2px 4px 2px 10px;
  background: var(--toolbar);
  border: 1px solid var(--border);
  border-radius: 6px;
  font-size: 11px;
}

.name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
