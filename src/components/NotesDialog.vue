<script setup lang="ts">
import { reactive, useTemplateRef } from 'vue'
import type { Session } from '../services/Session'

// The notes for the AI about the current database: the file `claude.md`. The AI adds facts to it too.
const props = defineProps<{ session: Session }>()
const dialog = useTemplateRef<HTMLDialogElement>('dialog')
const form = reactive({ database: '', path: '', text: '' })

/** Loads the notes, then shows the dialog. */
async function open(): Promise<void> {
  const notes = await props.session.chat.readNotes(props.session.id)
  form.database = notes.database
  form.path = notes.path
  form.text = notes.text
  // The native dialog closes on Escape by itself. That is Cancel.
  dialog.value?.showModal()
}

function close(): void {
  dialog.value?.close()
}

async function save(): Promise<void> {
  await props.session.chat.saveNotes(props.session.id, form.text)
  close()
}

function submit(): void {
  props.session.run(save())
}

defineExpose({ open })
</script>

<template>
  <dialog ref="dialog" class="notes-dialog">
    <form class="panel" @submit.prevent="submit">
      <div>
        <h2>Notes for {{ form.database }}</h2>
        <p class="path mono" :title="form.path">{{ form.path }}</p>
      </div>
      <textarea
        v-model="form.text"
        rows="14"
        spellcheck="false"
        placeholder="Tell the AI about this database. Example: orders.status 1 = pending, 2 = in progress, 3 = done. The AI adds notes here too."
      />
      <div class="actions">
        <button type="button" @click="close">Cancel</button>
        <button type="submit" class="primary">Save</button>
      </div>
    </form>
  </dialog>
</template>

<style scoped>
.notes-dialog {
  width: min(560px, 90vw);
  padding: 0;
  background: var(--toolbar);
  border: 1px solid var(--border);
  border-radius: 10px;
  box-shadow: 0 12px 32px #00000040;
  color: var(--text);
}

.notes-dialog::backdrop {
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
  overflow: hidden;
  font-size: 13px;
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.path {
  margin: 2px 0 0;
  overflow: hidden;
  color: var(--text-3);
  font-size: 10px;
  text-overflow: ellipsis;
  white-space: nowrap;
  -webkit-user-select: text;
  user-select: text;
}

textarea {
  padding: 6px 8px;
  border-radius: 5px;
  font-family: var(--font-mono);
  font-size: 11px;
  resize: vertical;
}

.actions {
  display: flex;
  gap: 8px;
  justify-content: flex-end;
}
</style>
