<script setup lang="ts">
import { reactive, useTemplateRef } from 'vue'
import {
  DEFAULT_FOLDER_COLOR,
  DEFAULT_FOLDER_ICON,
  FOLDER_COLORS,
  FOLDER_ICONS,
  FOLDER_NAME_MAX_LENGTH,
} from '../constants/folders'
import type { FolderColor, FolderIconName } from '../constants/folders'
import type { Folder } from '../services/FavoriteStore'
import { FAVORITE_STORE, injectRequired } from '../services/keys'
import FolderIcon from './FolderIcon.vue'

// The dialog to create a folder, or to edit one.
const store = injectRequired(FAVORITE_STORE)
const dialog = useTemplateRef<HTMLDialogElement>('dialog')

// `id` is the id of the folder that is edited. '' means a new folder.
const form = reactive({
  id: '',
  name: '',
  color: DEFAULT_FOLDER_COLOR as FolderColor,
  icon: DEFAULT_FOLDER_ICON as FolderIconName,
})

/** Opens the dialog. Pass a folder to edit it. Pass null for a new folder. */
function open(folder: Folder | null): void {
  form.id = folder?.id ?? ''
  form.name = folder?.name ?? ''
  form.color = folder?.color ?? DEFAULT_FOLDER_COLOR
  form.icon = folder?.icon ?? DEFAULT_FOLDER_ICON
  // The native dialog closes on Escape by itself.
  dialog.value?.showModal()
}

function close(): void {
  dialog.value?.close()
}

/** The dialog has no padding. A click on the dialog itself is a click outside the panel. */
function closeOnOutsideClick(event: MouseEvent): void {
  if (event.target === dialog.value) {
    close()
  }
}

/** A name of only spaces is not saved. The dialog then stays open. */
function submit(): void {
  if (store.saveFolder(form.id, form.name, form.color, form.icon)) {
    close()
  }
}

defineExpose({ open })
</script>

<template>
  <dialog ref="dialog" class="folder-dialog" @click="closeOnOutsideClick">
    <form class="panel" @submit.prevent="submit">
      <h2>{{ form.id ? 'Edit folder' : 'New folder' }}</h2>
      <div class="row">
        <label for="folder-name">Name</label>
        <input
          id="folder-name"
          v-model="form.name"
          required
          autofocus
          :maxlength="FOLDER_NAME_MAX_LENGTH"
          spellcheck="false"
        />
      </div>
      <div class="row">
        <label>Colour</label>
        <div class="choices">
          <button
            v-for="color in FOLDER_COLORS"
            :key="color"
            type="button"
            class="choice"
            :class="{ active: form.color === color }"
            :title="color"
            :aria-pressed="form.color === color"
            @click="form.color = color"
          >
            <span class="color" :style="{ background: `var(--folder-${color})` }" />
          </button>
        </div>
      </div>
      <div class="row top">
        <label>Icon</label>
        <div class="choices">
          <button
            v-for="icon in FOLDER_ICONS"
            :key="icon"
            type="button"
            class="choice"
            :class="{ active: form.icon === icon }"
            :title="icon"
            :aria-pressed="form.icon === icon"
            @click="form.icon = icon"
          >
            <FolderIcon :icon="icon" :color="form.color" :size="14" />
          </button>
        </div>
      </div>
      <div class="actions">
        <button type="button" @click="close">Cancel</button>
        <button type="submit" class="primary">{{ form.id ? 'Save' : 'Create folder' }}</button>
      </div>
    </form>
  </dialog>
</template>

<style scoped>
.folder-dialog {
  width: 320px;
  padding: 0;
  background: var(--toolbar);
  border: 1px solid var(--border);
  border-radius: 10px;
  box-shadow: 0 12px 32px #00000040;
  color: var(--text);
}

.folder-dialog::backdrop {
  background: #00000040;
}

.panel {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 14px 16px 16px;
}

h2 {
  margin: 0;
  font-size: 13px;
  font-weight: 600;
}

.row {
  display: flex;
  gap: 10px;
  align-items: center;
}

.row.top {
  align-items: flex-start;
}

.row label {
  flex: none;
  width: 44px;
  color: var(--text-2);
  line-height: 26px;
}

.row input {
  flex: 1;
  min-width: 0;
}

/* Six in a row: the colours fill one row, the icons fill two. */
.choices {
  display: grid;
  grid-template-columns: repeat(6, 26px);
  gap: 4px;
}

.choice {
  width: 26px;
  height: 26px;
  padding: 0;
  border-color: transparent;
  background: none;
}

.choice:hover {
  background: var(--bg);
}

.choice.active {
  background: var(--bg);
  border-color: var(--accent);
}

.color {
  width: 12px;
  height: 12px;
  border-radius: 50%;
}

.actions {
  display: flex;
  gap: 8px;
  justify-content: flex-end;
  padding-top: 4px;
}
</style>
