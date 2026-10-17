<script setup lang="ts">
import { nextTick, reactive, useTemplateRef } from 'vue'
import { confirm } from '@tauri-apps/plugin-dialog'
import { SAVED_QUERY_NAME_MAX_LENGTH } from '../constants/history'
import { SAVED_QUERIES, injectRequired } from '../services/keys'

// The dialog to save the editor SQL under a name, or to rename a saved query.
const props = defineProps<{ connectionKey: string }>()
const saved = injectRequired(SAVED_QUERIES)
const dialog = useTemplateRef<HTMLDialogElement>('dialog')
const nameInput = useTemplateRef<HTMLInputElement>('nameInput')

// `renaming` is the old name of the query that is renamed. '' means a new save of `sql`.
const form = reactive({ renaming: '', sql: '', name: '', problem: '' })

/** Opens the dialog to save the SQL under a new name. */
function save(sql: string): Promise<void> {
  return open('', sql)
}

/** Opens the dialog to rename a saved query. */
function rename(name: string): Promise<void> {
  return open(name, '')
}

async function open(renaming: string, sql: string): Promise<void> {
  form.renaming = renaming
  form.sql = sql
  form.name = renaming
  form.problem = ''
  // The native dialog closes on Escape by itself.
  dialog.value?.showModal()
  // Select the old name, so typing replaces it.
  await nextTick()
  nameInput.value?.select()
}

function close(): void {
  dialog.value?.close()
}

/** The dialog stays open when the name is taken (rename) or the user does not overwrite. */
async function submit(): Promise<void> {
  const key = props.connectionKey
  const name = form.name.trim()
  if (form.renaming) {
    if (saved.rename(key, form.renaming, name)) {
      close()
    } else {
      form.problem = 'Another saved query has this name.'
    }
    return
  }
  if (saved.has(key, name)) {
    const sure = await confirm(`A saved query named "${name}" exists. Replace its SQL?`, {
      title: 'Replace saved query',
      kind: 'warning',
    })
    if (!sure) {
      return
    }
  }
  if (saved.save(key, name, form.sql)) {
    close()
  }
}

defineExpose({ save, rename })
</script>

<template>
  <dialog ref="dialog" class="saved-query-dialog">
    <form class="panel" @submit.prevent="submit">
      <h2>{{ form.renaming ? `Rename "${form.renaming}"` : 'Save query' }}</h2>
      <input
        ref="nameInput"
        v-model="form.name"
        placeholder="Name"
        required
        autofocus
        :maxlength="SAVED_QUERY_NAME_MAX_LENGTH"
        spellcheck="false"
      />
      <p v-if="form.problem" class="error">{{ form.problem }}</p>
      <div class="actions">
        <button type="button" @click="close">Cancel</button>
        <button type="submit" class="primary">{{ form.renaming ? 'Rename' : 'Save' }}</button>
      </div>
    </form>
  </dialog>
</template>

<style scoped>
.saved-query-dialog {
  width: 320px;
  padding: 0;
  background: var(--toolbar);
  border: 1px solid var(--border);
  border-radius: 10px;
  box-shadow: 0 12px 32px #00000040;
  color: var(--text);
}

.saved-query-dialog::backdrop {
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
  overflow: hidden;
  font-size: 13px;
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.error {
  margin: 0;
}

.actions {
  display: flex;
  gap: 8px;
  justify-content: flex-end;
}
</style>
