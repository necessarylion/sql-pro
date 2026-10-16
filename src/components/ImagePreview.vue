<script setup lang="ts">
import { computed, ref, useTemplateRef } from 'vue'
import { XIcon } from 'lucide-vue-next'
import { AiAttachments } from '../services/AiAttachments'

/** The image to show: its name, type and base64 data. */
interface PreviewImage {
  name: string
  mediaType: string
  data: string
}

const dialog = useTemplateRef<HTMLDialogElement>('dialog')
const image = ref<PreviewImage | null>(null)
const caption = computed(captionText)

/** Like "photo.png · 240 KB". Base64 holds 3 bytes in 4 characters. */
function captionText(): string {
  if (!image.value) {
    return ''
  }
  const bytes = Math.round((image.value.data.length * 3) / 4)
  const size =
    bytes >= 1024 * 1024
      ? `${(bytes / 1024 / 1024).toFixed(1)} MB`
      : `${Math.ceil(bytes / 1024)} KB`
  return `${image.value.name} · ${size}`
}

/** The native dialog closes on Escape by itself. */
function open(file: PreviewImage): void {
  image.value = file
  dialog.value?.showModal()
}

function close(): void {
  dialog.value?.close()
}

/** A click on the dark area around the image closes it. */
function closeOnOutsideClick(event: MouseEvent): void {
  const target = event.target as HTMLElement
  if (target === dialog.value || target.classList.contains('frame')) {
    close()
  }
}

function forget(): void {
  image.value = null
}

defineExpose({ open })
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
    <div v-if="image" class="frame">
      <img :src="AiAttachments.preview(image)" :alt="image.name" />
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
