<script setup lang="ts">
import { onBeforeUnmount, ref } from 'vue'
import type { SectionHeight } from '../constants/layout'
import { LAYOUT_SERVICE, injectRequired } from '../services/keys'

// The drag handle on the bottom edge of one Structure view section. Works like PaneResizer.
// The section is the parent element. It must have `position: relative`.
// Its parent holds all sections. The drag keeps room there for the other sections.
const props = defineProps<{ size: SectionHeight }>()

const layout = injectRequired(LAYOUT_SERVICE)
const dragging = ref(false)

/** The classes on the root keep the resize cursor and stop text selection. See style.css. */
function setDragging(on: boolean): void {
  dragging.value = on
  document.documentElement.classList.toggle('resizing', on)
  document.documentElement.classList.toggle('rows', on)
}

/** The capture sends all moves to the handle, also when the mouse leaves it. */
function start(event: PointerEvent): void {
  ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
  setDragging(true)
}

/** The room left when every other section is as small as it can be. A closed section is its heading. */
function room(section: HTMLElement): number {
  const all = section.parentElement
  if (!all) {
    return Infinity
  }
  let others = 0
  for (const other of all.children) {
    if (other !== section && other instanceof HTMLElement && other.classList.contains('part')) {
      // An open section can shrink to its min height. A closed one is only its heading.
      const open = other.classList.contains('open')
      others += open ? parseFloat(getComputedStyle(other).minHeight) : other.offsetHeight
    }
  }
  return all.clientHeight - others
}

/** The new height is the way from the top of the section to the mouse. */
function move(event: PointerEvent): void {
  const section = (event.currentTarget as HTMLElement).parentElement
  if (!dragging.value || !section) {
    return
  }
  const height = event.clientY - section.getBoundingClientRect().top
  layout.resizeSection(props.size, height, room(section))
}

function stop(): void {
  if (dragging.value) {
    setDragging(false)
    layout.saveSection(props.size)
  }
}

// The section can close or go away in the middle of a drag.
onBeforeUnmount(stop)
</script>

<template>
  <div
    class="resizer"
    :class="{ dragging }"
    role="separator"
    aria-orientation="horizontal"
    title="Drag to resize. Double click to fit the content."
    @pointerdown.left="start"
    @pointermove="move"
    @pointerup="stop"
    @pointercancel="stop"
    @dblclick="layout.resetSection(size)"
  />
</template>

<style scoped>
/* 6px high. Its middle lies on the line between this section and the next. */
.resizer {
  position: absolute;
  right: 0;
  bottom: -3px;
  left: 0;
  z-index: 5;
  height: 6px;
  cursor: row-resize;
}

/* The accent line covers the separator line. */
.resizer:hover::after,
.resizer.dragging::after {
  position: absolute;
  inset: 2px 0;
  background: var(--accent);
  content: '';
}
</style>
