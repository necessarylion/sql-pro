<script setup lang="ts">
import { useTemplateRef } from 'vue'
import { XIcon } from 'lucide-vue-next'
import { THEME_LABELS } from '../constants/theme'
import { THEME_SERVICE, injectRequired } from '../services/keys'

const theme = injectRequired(THEME_SERVICE)
const dialog = useTemplateRef<HTMLDialogElement>('dialog')

/** The native dialog closes on Escape by itself. */
function open(): void {
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

defineExpose({ open })
</script>

<template>
  <dialog ref="dialog" class="settings" @click="closeOnOutsideClick">
    <div class="panel">
      <header>
        <h2>Settings</h2>
        <button class="icon" title="Close" @click="close">
          <XIcon :size="13" />
        </button>
      </header>
      <div class="row">
        <span>Theme</span>
        <div class="segmented">
          <button
            v-for="(text, choice) in THEME_LABELS"
            :key="choice"
            :class="{ active: theme.choice === choice }"
            @click="theme.choose(choice)"
          >
            {{ text }}
          </button>
        </div>
      </div>
    </div>
  </dialog>
</template>

<style scoped>
.settings {
  width: 340px;
  padding: 0;
  background: var(--toolbar);
  border: 1px solid var(--border);
  border-radius: 10px;
  box-shadow: 0 12px 32px #00000040;
  color: var(--text);
}

.settings::backdrop {
  background: #00000040;
}

.panel {
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding: 12px 14px 16px;
}

header,
.row {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

h2 {
  margin: 0;
  font-size: 13px;
  font-weight: 600;
}

header button {
  color: var(--text-2);
}

.segmented button {
  height: 22px;
  border-radius: 5px;
}
</style>
