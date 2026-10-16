<script setup lang="ts">
import type { Component } from 'vue'
import { ChevronDownIcon, ChevronRightIcon } from 'lucide-vue-next'

// The title of one section of the Structure view. A click opens or closes the section.
defineProps<{
  title: string
  count: number
  open: boolean
  icon: Component
  /** A CSS colour for the kind icon, like a token: var(--folder-blue). */
  iconColor?: string
  /** Draws a line above the heading, between two sections. */
  split?: boolean
}>()
const emit = defineEmits<{ toggle: [] }>()

function toggle(): void {
  emit('toggle')
}
</script>

<template>
  <button class="heading" :class="{ split }" :aria-expanded="open" @click="toggle">
    <component :is="open ? ChevronDownIcon : ChevronRightIcon" :size="13" />
    <component :is="icon" :size="13" :style="{ color: iconColor }" />
    <span class="title">{{ title }}</span>
    <span class="count">{{ count }}</span>
  </button>
</template>

<style scoped>
/* Sits above the section body. Only the body scrolls. */
.heading {
  display: flex;
  flex: none;
  align-items: center;
  justify-content: flex-start;
  gap: 6px;
  width: 100%;
  height: 30px;
  padding: 0 10px;
  border: none;
  border-radius: 0;
  background: var(--bg);
  color: var(--text-3);
  font-size: 13px;
  text-align: left;
  cursor: pointer;
}

.heading.split {
  border-top: 1px solid var(--border);
}

.title {
  color: var(--text);
  font-weight: 600;
}

.count {
  padding: 0 6px;
  border-radius: 999px;
  background: var(--header);
  color: var(--text-2);
  font-size: 11px;
  line-height: 16px;
}
</style>
