<script setup lang="ts">
import type { Session } from '../services/Session'

// The bottom bar of a view. The default slot is the left side, `end` is the right side.
// Between them it shows the error, the busy text or the last status of the session.
defineProps<{ session: Session }>()
</script>

<template>
  <div class="bar bottom">
    <slot />
    <span v-if="session.error" class="status error" :title="session.error">
      {{ session.error }}
    </span>
    <span v-else class="status dim" :title="session.status">
      {{ session.busy ? 'Working…' : session.status }}
    </span>
    <slot name="end" />
  </div>
</template>

<style scoped>
/* The bar grows when a long error needs more lines. */
.bar.bottom {
  flex-wrap: wrap;
  row-gap: 3px;
  height: auto;
  min-height: 28px;
  padding-top: 2px;
  padding-bottom: 2px;
}

.status {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* An error is never cut. It wraps, and scrolls when it is very long. */
.status.error {
  flex-basis: 200px;
  max-height: 58px;
  overflow-y: auto;
  font-weight: 500;
  white-space: normal;
  overflow-wrap: anywhere;
}

:slotted(.divider) {
  width: 1px;
  height: 14px;
  background: var(--border);
}
</style>
