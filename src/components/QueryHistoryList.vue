<script setup lang="ts">
import { XIcon } from 'lucide-vue-next'
import { QUERY_HISTORY, injectRequired } from '../services/keys'
import type { QueryHistoryEntry } from '../services/QueryHistory'

const emit = defineEmits<{ pick: [sql: string] }>()
const history = injectRequired(QUERY_HISTORY)

/** The time of the run, in the date format of the user. */
function ranAt(entry: QueryHistoryEntry): string {
  return new Date(entry.ranAt).toLocaleString()
}
</script>

<template>
  <div class="history">
    <div class="section">
      <span>
        History <span class="count">{{ history.entries.length }}</span>
      </span>
      <button
        class="clear"
        title="Remove the whole history"
        :disabled="history.entries.length === 0"
        @click="history.clear()"
      >
        Clear
      </button>
    </div>
    <div v-if="history.entries.length === 0" class="empty">No history yet</div>
    <ul v-else class="list">
      <li v-for="entry in history.entries" :key="entry.id" @click="emit('pick', entry.sql)">
        <span class="dim">{{ ranAt(entry) }}</span>
        <span class="dim connection" :title="entry.connection">{{ entry.connection }}</span>
        <span class="name sql" :title="entry.sql">{{ entry.sql }}</span>
        <button class="icon" title="Remove from history" @click.stop="history.remove(entry.id)">
          <XIcon :size="11" />
        </button>
      </li>
    </ul>
  </div>
</template>

<style scoped>
.history {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-height: 0;
}

.clear {
  height: 20px;
  padding: 0 8px;
  font-size: 11px;
  text-transform: none;
}

.list {
  flex: 1;
  overflow-y: auto;
}

.list li {
  gap: 10px;
  font-size: 11px;
  cursor: pointer;
}

.list li:hover {
  background: var(--row-alt);
}

.connection {
  flex: none;
  max-width: 140px;
  overflow: hidden;
  text-overflow: ellipsis;
}

.sql {
  font-family: var(--font-mono);
}

.icon {
  color: var(--text-2);
}
</style>
