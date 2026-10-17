<script setup lang="ts">
import { DRIVER_LABELS } from '../constants'
import { DRIVER_LOGOS } from '../constants/logos'
import type { Driver } from '../types'

defineProps<{ driver: Driver }>()
</script>

<template>
  <span class="badge" :class="driver" role="img" :aria-label="DRIVER_LABELS[driver]">
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <title>{{ DRIVER_LABELS[driver] }}</title>
      <path :d="DRIVER_LOGOS[driver]" />
    </svg>
  </span>
</template>

<style scoped>
/* A square tile in the brand colour, the same size for every database. */
.badge {
  flex: none;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  border-radius: 4px;
}

.badge svg {
  width: 12px;
  height: 12px;
  fill: var(--on-accent);
}

.mysql {
  background: var(--mysql);
}

.postgres {
  background: var(--postgres);
}

.mssql {
  background: var(--mssql);
}

.sqlite {
  background: var(--sqlite);
}
</style>
