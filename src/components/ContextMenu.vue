<script setup lang="ts">
import { onMounted, reactive, useTemplateRef } from 'vue'

// A small right-click menu. It looks and closes like the table menu of the workspace.
// The parent shows it with `v-if` and puts `<button role="menuitem">` items in it.
// A `<span class="menu-line" />` draws a line between items.
const props = defineProps<{ x: number; y: number }>()
const emit = defineEmits<{ close: [] }>()

const panel = useTemplateRef<HTMLElement>('panel')
const place = reactive({ left: props.x, top: props.y })

onMounted(fit)

/** Keeps the menu inside the window. It takes the focus, so Escape closes it. */
function fit(): void {
  const menu = panel.value
  if (menu) {
    const box = menu.getBoundingClientRect()
    place.left = Math.max(4, Math.min(props.x, window.innerWidth - box.width - 4))
    place.top = Math.max(4, Math.min(props.y, window.innerHeight - box.height - 4))
    menu.focus()
  }
}

function close(): void {
  emit('close')
}
</script>

<template>
  <!-- The backdrop covers the window. Any click closes the menu, also a click on an item. -->
  <div class="menu-backdrop" @click="close" @contextmenu.prevent="close" @wheel="close">
    <div
      ref="panel"
      class="menu"
      role="menu"
      tabindex="-1"
      :style="{ left: `${place.left}px`, top: `${place.top}px` }"
      @keydown.esc="close"
    >
      <slot />
    </div>
  </div>
</template>

<style scoped>
.menu-backdrop {
  position: fixed;
  inset: 0;
  z-index: 100;
}

/* Looks like a native macOS menu. */
.menu {
  position: fixed;
  display: flex;
  flex-direction: column;
  min-width: 170px;
  max-width: 280px;
  max-height: calc(100vh - 8px);
  padding: 5px;
  overflow-y: auto;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 6px;
  outline: none;
  box-shadow: 0 6px 20px #00000040;
}

.menu :slotted(button) {
  display: block;
  flex: none;
  height: 22px;
  padding: 0 8px;
  overflow: hidden;
  border: 0;
  border-radius: 4px;
  background: none;
  color: var(--text);
  font-size: 12px;
  font-weight: 400;
  text-align: left;
  text-overflow: ellipsis;
}

.menu :slotted(button.danger) {
  color: var(--danger);
}

.menu :slotted(.menu-line) {
  flex: none;
  height: 1px;
  margin: 4px 0;
  background: var(--border);
}

.menu :slotted(button:hover),
.menu :slotted(button:focus-visible) {
  background: var(--accent);
  color: var(--on-accent);
  outline: none;
}
</style>
