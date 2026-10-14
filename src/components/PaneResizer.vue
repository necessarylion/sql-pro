<script setup lang="ts">
import { onBeforeUnmount, ref } from 'vue'
import type { LayoutSize } from '../constants/layout'
import { LAYOUT_SERVICE, injectRequired } from '../services/keys'

// The drag handle on the edge of a pane. It lies on the right edge for the sidebar
// width and on the bottom edge for the editor height. The pane is the parent
// element. It must have `position: relative`.
const props = defineProps<{ size: LayoutSize }>()

const layout = injectRequired(LAYOUT_SERVICE)
const dragging = ref(false)
/** True when the handle is a horizontal line that moves up and down. */
const rows = props.size === 'editorHeight'

/** The classes on the root keep the resize cursor and stop text selection. See style.css. */
function setDragging(on: boolean): void {
  dragging.value = on
  document.documentElement.classList.toggle('resizing', on)
  document.documentElement.classList.toggle('rows', on && rows)
}

/** The capture sends all moves to the handle, also when the mouse leaves it. */
function start(event: PointerEvent): void {
  ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
  setDragging(true)
}

/** The new size is the way from the start of the pane to the mouse. */
function move(event: PointerEvent): void {
  const pane = (event.currentTarget as HTMLElement).parentElement
  if (dragging.value && pane) {
    const box = pane.getBoundingClientRect()
    layout.resize(rows ? event.clientY - box.top : event.clientX - box.left, props.size)
  }
}

function stop(): void {
  if (dragging.value) {
    setDragging(false)
    layout.save(props.size)
  }
}

// The handle can go away in the middle of a drag, when its tab connects.
onBeforeUnmount(stop)
</script>

<template>
  <div
    class="resizer"
    :class="{ dragging, rows }"
    role="separator"
    :aria-orientation="rows ? 'horizontal' : 'vertical'"
    title="Drag to resize. Double click to reset."
    @pointerdown.left="start"
    @pointermove="move"
    @pointerup="stop"
    @pointercancel="stop"
    @dblclick="layout.reset(size)"
  />
</template>

<style scoped>
/* 5px wide. Its middle lies on the 1px border of the pane. */
.resizer {
  position: absolute;
  top: 0;
  right: -2px;
  bottom: 0;
  z-index: 5;
  width: 5px;
  cursor: col-resize;
}

.resizer.rows {
  top: auto;
  right: 0;
  bottom: -2px;
  left: 0;
  width: auto;
  height: 5px;
  cursor: row-resize;
}

/* The accent line covers the border. */
.resizer:hover::after,
.resizer.dragging::after {
  position: absolute;
  inset: 0 2px;
  background: var(--accent);
  content: '';
}

.resizer.rows::after {
  inset: 2px 0;
}
</style>
