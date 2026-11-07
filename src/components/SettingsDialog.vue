<script setup lang="ts">
import { useTemplateRef } from 'vue'
import { XIcon } from 'lucide-vue-next'
import { FONT_SIZE_MAX, FONT_SIZE_MIN } from '../constants/fonts'
import { THEME_LABELS } from '../constants/theme'
import { FONT_SERVICE, THEME_SERVICE, injectRequired } from '../services/keys'

const theme = injectRequired(THEME_SERVICE)
const fonts = injectRequired(FONT_SERVICE)
const dialog = useTemplateRef<HTMLDialogElement>('dialog')

/** The native dialog closes on Escape by itself. */
function open(): void {
  dialog.value?.showModal()
  // The list is only needed here. Without it, the pickers still show the saved fonts.
  void fonts.loadFonts().catch(console.error)
}

function pickedName(event: Event): string {
  return (event.target as HTMLSelectElement).value
}

function chooseUiFont(event: Event): void {
  fonts.chooseUiFont(pickedName(event))
}

function chooseMonoFont(event: Event): void {
  fonts.chooseMonoFont(pickedName(event))
}

/** A size out of range is moved into range. The field then shows the size that is used. */
function chooseSize(event: Event): void {
  const input = event.target as HTMLInputElement
  fonts.chooseSize(input.valueAsNumber)
  input.value = String(fonts.size)
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
      <label class="row">
        <span>Font</span>
        <select :value="fonts.uiFont" @change="chooseUiFont">
          <option value="">Default</option>
          <option v-if="fonts.uiFont && !fonts.fonts.includes(fonts.uiFont)" :value="fonts.uiFont">
            {{ fonts.uiFont }}
          </option>
          <option v-for="name in fonts.fonts" :key="name" :value="name">{{ name }}</option>
        </select>
      </label>
      <label class="row">
        <span>Code font</span>
        <select :value="fonts.monoFont" @change="chooseMonoFont">
          <option value="">Default</option>
          <option
            v-if="fonts.monoFont && !fonts.fonts.includes(fonts.monoFont)"
            :value="fonts.monoFont"
          >
            {{ fonts.monoFont }}
          </option>
          <option v-for="name in fonts.fonts" :key="name" :value="name">{{ name }}</option>
        </select>
      </label>
      <label class="row">
        <span>Font size</span>
        <input
          type="number"
          :min="FONT_SIZE_MIN"
          :max="FONT_SIZE_MAX"
          :value="fonts.size"
          @change="chooseSize"
        />
      </label>
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
  font-size: calc(13px * var(--font-scale));
  font-weight: 600;
}

header button {
  color: var(--text-2);
}

select {
  width: 180px;
}

input[type='number'] {
  width: 56px;
}

.segmented button {
  height: 22px;
  border-radius: 5px;
}
</style>
