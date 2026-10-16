<script setup lang="ts">
import { DRIVER_BADGES } from '../constants'
import type { Driver } from '../types'

defineProps<{ driver: Driver }>()
</script>

<template>
  <span class="badge" :class="driver">{{ DRIVER_BADGES[driver] }}</span>
</template>

<style scoped>
.badge {
  flex: none;
  height: 15px;
  padding: 0 4px;
  border-radius: 3px;
  color: var(--on-accent);
  font-size: 9px;
  font-weight: 700;
  line-height: 15px;
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
