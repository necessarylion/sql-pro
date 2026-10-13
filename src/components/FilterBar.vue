<script setup lang="ts">
import { MinusIcon, PlusIcon } from 'lucide-vue-next'
import {
  FILTER_OPERATOR_LABELS,
  RAW_FILTER_COLUMN,
  VALUELESS_OPERATORS,
} from '../constants/filters'
import type { Session } from '../services/Session'
import type { FilterRow } from '../types'

// The filter rows over the content grid. The rows live in the session.
const props = defineProps<{ session: Session }>()

function isRaw(row: FilterRow): boolean {
  return row.column === RAW_FILTER_COLUMN
}

function needsNoValue(row: FilterRow): boolean {
  return !isRaw(row) && VALUELESS_OPERATORS.includes(row.operator)
}

/** Applies all ticked rows, not only the row of the button. */
function apply(): void {
  props.session.run(props.session.applyFilters())
}
</script>

<template>
  <div class="filters">
    <div v-for="(row, index) in session.filterRows" :key="index" class="bar row">
      <input v-model="row.enabled" type="checkbox" title="Use this rule" />
      <select v-model="row.column" class="column" title="Column">
        <option v-for="column in session.page?.columns" :key="column.name">
          {{ column.name }}
        </option>
        <option :value="RAW_FILTER_COLUMN">Raw SQL (WHERE)</option>
      </select>
      <select v-if="!isRaw(row)" v-model="row.operator" class="operator" title="Operator">
        <option
          v-for="(text, operator) in FILTER_OPERATOR_LABELS"
          :key="operator"
          :value="operator"
        >
          {{ text }}
        </option>
      </select>
      <input
        v-model="row.value"
        class="value"
        :class="{ mono: isRaw(row) }"
        :disabled="needsNoValue(row)"
        :placeholder="isRaw(row) ? `status = 'active' AND id > 100` : ''"
        autocapitalize="off"
        spellcheck="false"
        @keydown.enter="apply"
      />
      <button title="Apply all ticked rules" @click="apply">Apply</button>
      <button
        class="icon"
        title="Remove this rule"
        @click="session.run(session.removeFilterRow(index))"
      >
        <MinusIcon :size="13" />
      </button>
      <button class="icon" title="Add a rule below" @click="session.addFilterRow(index)">
        <PlusIcon :size="13" />
      </button>
    </div>
  </div>
</template>

<style scoped>
/* All rows share one border. Many rows scroll. */
.filters {
  flex: none;
  max-height: 150px;
  padding: 3px 0;
  overflow-y: auto;
  background: var(--toolbar);
  border-bottom: 1px solid var(--border);
}

.bar.row {
  height: 28px;
  border-bottom: 0;
}

input[type='checkbox'] {
  flex: none;
  width: 13px;
  height: 13px;
  margin: 0;
  padding: 0;
  accent-color: var(--accent);
}

select {
  flex: none;
  height: 22px;
  padding: 0 4px;
  border-radius: 5px;
}

.column {
  width: 150px;
}

.operator {
  width: 110px;
}

.value {
  flex: 1;
  min-width: 0;
}

.value:disabled {
  opacity: 0.5;
}
</style>
