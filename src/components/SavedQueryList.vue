<script setup lang="ts">
import { confirm } from '@tauri-apps/plugin-dialog'
import { PencilIcon, XIcon } from 'lucide-vue-next'
import { SAVED_QUERIES, injectRequired } from '../services/keys'

// The saved queries of the server of the tab. A click puts the SQL into the editor.
const props = defineProps<{ connectionKey: string }>()
const emit = defineEmits<{ pick: [sql: string]; rename: [name: string] }>()
const saved = injectRequired(SAVED_QUERIES)

async function remove(name: string): Promise<void> {
  const sure = await confirm(`Delete the saved query "${name}"?`, {
    title: 'Delete saved query',
    kind: 'warning',
  })
  if (sure) {
    saved.remove(props.connectionKey, name)
  }
}
</script>

<template>
  <div class="saved">
    <div class="section">
      <span>
        Saved <span class="count">{{ saved.list(connectionKey).length }}</span>
      </span>
    </div>
    <div v-if="saved.list(connectionKey).length === 0" class="empty">
      No saved queries. Use Save to add one.
    </div>
    <ul v-else class="list">
      <li
        v-for="query in saved.list(connectionKey)"
        :key="query.name"
        :title="query.sql"
        @click="emit('pick', query.sql)"
      >
        <span class="title">{{ query.name }}</span>
        <span class="name sql dim">{{ query.sql }}</span>
        <button class="icon" title="Rename" @click.stop="emit('rename', query.name)">
          <PencilIcon :size="11" />
        </button>
        <button class="icon" title="Delete" @click.stop="remove(query.name)">
          <XIcon :size="11" />
        </button>
      </li>
    </ul>
  </div>
</template>

<style scoped>
/* Takes at most half of the area. The history gets the rest. */
.saved {
  display: flex;
  flex: none;
  flex-direction: column;
  max-height: 50%;
  border-bottom: 1px solid var(--border);
}

.empty {
  flex: none;
  height: 28px;
  font-size: 11px;
}

.list {
  overflow-y: auto;
}

.list li {
  gap: 10px;
  font-size: 11px;
}

.list li:hover {
  background: var(--row-alt);
}

.title {
  flex: none;
  max-width: 200px;
  overflow: hidden;
  font-weight: 600;
  text-overflow: ellipsis;
}

.sql {
  font-family: var(--font-mono);
}

.icon {
  color: var(--text-2);
}
</style>
