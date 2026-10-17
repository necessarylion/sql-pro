<script setup lang="ts">
import { save } from '@tauri-apps/plugin-dialog'
import type { Session } from '../services/Session'
import type { ExportFormat, Row, TableRef } from '../types'
import { SHORTCUTS, injectRequired } from '../services/keys'
import ContextMenu from './ContextMenu.vue'

// The menu that exports grid rows to a file. The parent picks the rows.
// `table` is null for a query result. Then the SQL file name is the table name.
const props = defineProps<{
  session: Session
  x: number
  y: number
  columns: string[]
  rows: Row[]
  table: TableRef | null
  /** Shows a Copy item first. A click on it sends `copy`: the parent copies the selected grid rows. */
  copies?: boolean
}>()
const emit = defineEmits<{ close: []; copy: [] }>()
const shortcuts = injectRequired(SHORTCUTS)

// A click on an item also closes the menu. So the values are read here, before that.
function exportAs(format: ExportFormat): void {
  const { session, table, columns, rows } = props
  session.run(write(format, session, table, columns, rows))
}

async function write(
  format: ExportFormat,
  session: Session,
  table: TableRef | null,
  columns: string[],
  rows: Row[],
): Promise<void> {
  const kind = format.toUpperCase()
  const path = await save({
    title: table ? `Export rows as ${kind}` : `Export rows as ${kind} (file name = table name)`,
    defaultPath: session.rowsName(table, format),
    filters: [{ name: kind, extensions: [format] }],
  })
  if (path) {
    await session.exportRows(format, table, columns, rows, path)
  }
}

function copy(): void {
  emit('copy')
}

function close(): void {
  emit('close')
}
</script>

<template>
  <ContextMenu :x="x" :y="y" @close="close">
    <template v-if="copies">
      <button role="menuitem" @click="copy">
        Copy
        <span class="hint">{{ shortcuts.hint('C') }}</span>
      </button>
      <span class="menu-line" />
    </template>
    <!-- The parent can put its own items above the export items. -->
    <slot />
    <button role="menuitem" :disabled="rows.length === 0" @click="exportAs('csv')">
      Export {{ rows.length.toLocaleString() }} {{ rows.length === 1 ? 'row' : 'rows' }} as CSV…
    </button>
    <button role="menuitem" :disabled="rows.length === 0" @click="exportAs('sql')">
      Export {{ rows.length.toLocaleString() }} {{ rows.length === 1 ? 'row' : 'rows' }} as SQL…
    </button>
  </ContextMenu>
</template>
