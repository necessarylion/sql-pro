<script setup lang="ts">
import { onMounted, reactive, useTemplateRef } from 'vue'
import { FieldValue, type PickerType } from '../services/FieldValue'

// A small popover under a date or time field of the row detail panel.
// It shows visible native inputs: WebKit has no time popup, so the user types or steps the time here.
// It sits on the body with position: fixed, so the scrolling field list never cuts it off.
const props = defineProps<{
  picker: PickerType
  /** The current database text of the field. */
  value: string | null
  /** The field the popover opens under. */
  anchor: HTMLElement
}>()

const emit = defineEmits<{
  /** The chosen value as database text. */
  set: [value: string]
  close: []
}>()

const panel = useTemplateRef<HTMLElement>('panel')
const parts = reactive(FieldValue.toPickerParts(props.value, props.picker))
const place = reactive({ left: 0, top: 0 })

onMounted(fit)

/** Puts the popover under the field, or above it when there is no room below. Focuses the first input. */
function fit(): void {
  const box = props.anchor.getBoundingClientRect()
  const own = panel.value?.getBoundingClientRect()
  const width = own?.width ?? 0
  const height = own?.height ?? 0
  place.left = Math.max(4, Math.min(box.left, window.innerWidth - width - 4))
  const below = box.bottom + 4
  place.top = below + height > window.innerHeight - 4 ? Math.max(4, box.top - height - 4) : below
  panel.value?.querySelector('input')?.focus()
}

function now(): void {
  Object.assign(parts, FieldValue.nowParts(new Date()))
}

/** Sends the value and closes. An empty input changes nothing. */
function set(): void {
  const text = FieldValue.fromPickerParts(parts, props.picker)
  if (text !== null) {
    emit('set', text)
  }
  emit('close')
}

/** Enter in an input sets the value. On a button, Enter keeps its own click. */
function enter(event: KeyboardEvent): void {
  if (event.target instanceof HTMLInputElement) {
    event.preventDefault()
    set()
  }
}

function close(): void {
  emit('close')
}
</script>

<template>
  <Teleport to="body">
    <!-- The backdrop catches a click outside. It closes without a change. -->
    <div class="popover-backdrop" @click.self="close" @wheel.self="close">
      <div
        ref="panel"
        class="popover"
        :style="{ left: `${place.left}px`, top: `${place.top}px` }"
        @keydown.enter="enter"
        @keydown.esc.prevent="close"
      >
        <div class="inputs">
          <input v-if="picker !== 'time'" v-model="parts.date" type="date" />
          <input v-if="picker !== 'date'" v-model="parts.time" type="time" step="1" />
        </div>
        <div class="buttons">
          <button @click="now">Now</button>
          <span class="spacer" />
          <button @click="close">Cancel</button>
          <button class="primary" @click="set">Set</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
/* Above the panels (z-index 3), below menus and dialogs (100). */
.popover-backdrop {
  position: fixed;
  inset: 0;
  z-index: 50;
}

.popover {
  position: fixed;
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 6px;
  background: var(--toolbar);
  border: 1px solid var(--border);
  border-radius: 6px;
  box-shadow: var(--popover-shadow);
  font-size: 11px;
}

.inputs,
.buttons {
  display: flex;
  gap: 4px;
  align-items: center;
}

input {
  height: 22px;
  font-family: var(--font-mono);
  font-size: 11px;
}

.spacer {
  flex: 1;
  min-width: 8px;
}

button {
  height: 22px;
  padding: 0 8px;
  border-radius: 5px;
  font-size: 11px;
}
</style>
