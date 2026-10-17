<script setup lang="ts">
import { nextTick, reactive, ref, useTemplateRef } from 'vue'
import { save } from '@tauri-apps/plugin-dialog'
import { ChevronRightIcon } from 'lucide-vue-next'
import { COPY_FORMAT_GROUPS, COPY_SUBMENU_WIDTH } from '../constants/copy'
import type { CopyFormatItem } from '../constants/copy'
import type { Session } from '../services/Session'
import type { CopyFormat, ExportFormat, Row, RowsPick, TableRef } from '../types'
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
  /** The selected grid rows. Given with `copies`, the menu shows "Copy Rows As". */
  picked?: RowsPick
}>()
const emit = defineEmits<{ close: []; copy: [] }>()
const shortcuts = injectRequired(SHORTCUTS)

// The "Copy Rows As" submenu. It is fixed on screen, so the scrolling menu does not cut it off.
const submenuOpen = ref(false)
const submenuPlace = reactive({ left: 0, top: 0 })
const submenu = useTemplateRef<HTMLElement>('submenu')

/** Opens the submenu next to its item, on the left when there is no room on the right. */
async function showSubmenu(event: Event): Promise<void> {
  const box = (event.currentTarget as HTMLElement).getBoundingClientRect()
  const right = box.right + COPY_SUBMENU_WIDTH <= window.innerWidth
  submenuPlace.left = right ? box.right : box.left - COPY_SUBMENU_WIDTH
  submenuPlace.top = box.top - 6
  submenuOpen.value = true
  await nextTick()
  const height = submenu.value?.getBoundingClientRect().height ?? 0
  submenuPlace.top = Math.max(4, Math.min(submenuPlace.top, window.innerHeight - height - 4))
}

function hideSubmenu(): void {
  submenuOpen.value = false
}

/** A click on a format copies the rows. The click also closes the menu, so values are read first. */
function copyAs(item: CopyFormatItem): void {
  const { session, table, picked } = props
  if (picked) {
    session.run(writeClipboard(session, item, table, picked))
  }
}

async function writeClipboard(
  session: Session,
  item: CopyFormatItem,
  table: TableRef | null,
  picked: RowsPick,
): Promise<void> {
  const format: CopyFormat = item.format
  await navigator.clipboard.writeText(await session.rowsText(format, table, picked))
  session.noteCopied(picked.rows.length, item.label)
}

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
      <div v-if="picked" class="has-submenu" @mouseenter="showSubmenu" @mouseleave="hideSubmenu">
        <button
          class="trigger"
          role="menuitem"
          @click.stop="showSubmenu"
          aria-haspopup="menu"
          :aria-expanded="submenuOpen"
          :class="{ open: submenuOpen }"
        >
          Copy Rows As
          <ChevronRightIcon class="arrow" :size="12" />
        </button>
        <div
          v-if="submenuOpen"
          ref="submenu"
          class="menu submenu"
          role="menu"
          :style="{
            left: `${submenuPlace.left}px`,
            top: `${submenuPlace.top}px`,
            width: `${COPY_SUBMENU_WIDTH}px`,
          }"
        >
          <template v-for="(group, index) in COPY_FORMAT_GROUPS" :key="index">
            <span v-if="index > 0" class="menu-line" />
            <button
              v-for="item in group"
              :key="item.format"
              role="menuitem"
              :disabled="picked.rows.length === 0"
              @click="copyAs(item)"
            >
              {{ item.label }}
            </button>
          </template>
        </div>
      </div>
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

<style scoped>
/* The "Copy Rows As" item reaches over the menu padding, so the mouse can cross to the submenu. */
.has-submenu {
  display: flex;
  flex-direction: column;
  margin: 0 -5px;
  padding: 0 5px;
}

/* A flex row like the Export and Import items of the workspace menu, so the arrow sits in the middle. */
.has-submenu > button.trigger {
  display: flex;
  align-items: center;
}

.arrow {
  margin-left: auto;
}

/* The same look as the menu around it. */
.submenu {
  position: fixed;
  z-index: 1;
  display: flex;
  flex-direction: column;
  padding: 5px;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 6px;
  box-shadow: 0 6px 20px #00000040;
}

.has-submenu button.open {
  background: var(--accent);
  color: var(--on-accent);
}
</style>
