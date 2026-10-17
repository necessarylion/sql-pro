<script setup lang="ts">
import { reactive, useTemplateRef } from 'vue'
import { MYSQL_COLLATIONS } from '../constants/databases'
import type { Session } from '../services/Session'

// The dialog to create a database. Charset and collation are only shown for MySQL.
const props = defineProps<{ session: Session }>()
const dialog = useTemplateRef<HTMLDialogElement>('dialog')
const form = reactive({ name: '', charset: '', collation: '' })

function open(): void {
  form.name = ''
  form.charset = ''
  form.collation = ''
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

const charsets = Object.keys(MYSQL_COLLATIONS)

/** The collations of the chosen charset. With no known charset, all common ones. */
function collations(): readonly string[] {
  return (
    MYSQL_COLLATIONS[form.charset.trim().toLowerCase()] ?? Object.values(MYSQL_COLLATIONS).flat()
  )
}

function isMysql(): boolean {
  return props.session.config?.driver === 'mysql'
}

/** Closes only when the database was created. On an error the tab shows it and the form stays. */
async function submit(): Promise<void> {
  const session = props.session
  if (await session.run(session.createDatabase(form.name, form.charset, form.collation))) {
    close()
  }
}

defineExpose({ open })
</script>

<template>
  <dialog ref="dialog" class="database-dialog" @click="closeOnOutsideClick">
    <form class="panel" @submit.prevent="submit">
      <h2>New database</h2>
      <div class="row">
        <label for="database-name">Name</label>
        <input id="database-name" v-model="form.name" required autofocus spellcheck="false" />
      </div>
      <template v-if="isMysql()">
        <div class="row">
          <label for="database-charset">Charset</label>
          <input
            id="database-charset"
            v-model="form.charset"
            list="database-charsets"
            placeholder="Server default"
            spellcheck="false"
          />
          <datalist id="database-charsets">
            <option v-for="charset in charsets" :key="charset" :value="charset" />
          </datalist>
        </div>
        <div class="row">
          <label for="database-collation">Collation</label>
          <input
            id="database-collation"
            v-model="form.collation"
            list="database-collations"
            placeholder="Server default"
            spellcheck="false"
          />
          <datalist id="database-collations">
            <option v-for="collation in collations()" :key="collation" :value="collation" />
          </datalist>
        </div>
      </template>
      <div class="actions">
        <button type="button" @click="close">Cancel</button>
        <button type="submit" class="primary">Create database</button>
      </div>
    </form>
  </dialog>
</template>

<style scoped>
.database-dialog {
  width: 320px;
  padding: 0;
  background: var(--toolbar);
  border: 1px solid var(--border);
  border-radius: 10px;
  box-shadow: 0 12px 32px #00000040;
  color: var(--text);
}

.database-dialog::backdrop {
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

.row label {
  flex: none;
  width: 60px;
  color: var(--text-2);
  line-height: 26px;
}

.row input {
  flex: 1;
  min-width: 0;
}

.actions {
  display: flex;
  gap: 8px;
  justify-content: flex-end;
  padding-top: 4px;
}
</style>
