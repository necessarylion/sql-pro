<script setup lang="ts">
import { computed } from 'vue'
import { Environment } from '../services/Environment'

const props = defineProps<{ name: string }>()

const clean = computed(cleanName)
const kind = computed(kindOfName)

function cleanName(): string {
  return Environment.clean(props.name)
}

function kindOfName(): string {
  return Environment.kind(props.name)
}
</script>

<template>
  <span v-if="clean" class="env-badge" :class="kind" :title="`Environment: ${clean}`">
    {{ clean }}
  </span>
</template>

<style scoped>
/* Every colour pair has its own solid background, so it reads the same on any row. */
.env-badge {
  flex: none;
  height: 15px;
  padding: 0 5px;
  overflow: hidden;
  border-radius: 3px;
  font-family: var(--font-ui);
  font-size: 9px;
  font-weight: 700;
  line-height: 15px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.local {
  background: color-mix(in srgb, var(--text-3) 30%, var(--bg));
  color: var(--text-2);
}

.dev {
  background: var(--stmt-highlight);
  color: var(--accent-text);
}

.qa {
  background: var(--modified);
  color: var(--warn);
}

.prod {
  background: var(--deleted);
  color: var(--danger);
}

/* Teal, so a custom name never looks like one of the four above. */
.custom {
  background: color-mix(in srgb, var(--syntax-type) 12%, var(--bg));
  color: var(--syntax-type);
}
</style>
