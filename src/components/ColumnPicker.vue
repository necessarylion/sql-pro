<script setup lang="ts">
import { computed } from 'vue'
import { XIcon } from 'lucide-vue-next'

/** Picks one or more names in order. The order is the order of picking. */
const picked = defineModel<string[]>({ required: true })
const props = defineProps<{ options: string[]; placeholder: string }>()
const free = computed(freeOptions)

/** The names that are not picked yet. */
function freeOptions(): string[] {
  const free: string[] = []
  for (const name of props.options) {
    if (!picked.value.includes(name)) {
      free.push(name)
    }
  }
  return free
}

function add(event: Event): void {
  const select = event.target as HTMLSelectElement
  if (select.value !== '') {
    picked.value = [...picked.value, select.value]
  }
  select.value = ''
}

function remove(index: number): void {
  picked.value = [...picked.value.slice(0, index), ...picked.value.slice(index + 1)]
}
</script>

<template>
  <span class="picker">
    <span v-for="(name, index) in picked" :key="name" class="chip mono">
      {{ name }}
      <button class="icon" :title="`Remove ${name}`" @click="remove(index)">
        <XIcon :size="10" />
      </button>
    </span>
    <select :title="placeholder" :disabled="free.length === 0" @change="add">
      <option value="">{{ placeholder }}</option>
      <option v-for="name in free" :key="name" :value="name">{{ name }}</option>
    </select>
  </span>
</template>

<style scoped>
.picker {
  display: inline-flex;
  gap: 4px;
  align-items: center;
}

.chip {
  display: inline-flex;
  gap: 2px;
  align-items: center;
  height: 20px;
  padding: 0 0 0 6px;
  background: var(--seg-active);
  border: 1px solid var(--border);
  border-radius: 4px;
  font-size: 11px;
}

.chip button.icon {
  width: 16px;
  height: 16px;
  color: var(--text-2);
}

select {
  height: 22px;
  padding: 0 4px;
  border-radius: 5px;
  font-size: 11px;
}
</style>
