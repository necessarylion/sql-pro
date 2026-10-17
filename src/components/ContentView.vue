<script setup lang="ts">
import { computed, ref, useTemplateRef, watch } from 'vue'
import {
  ChevronLeftIcon,
  ChevronRightIcon,
  DownloadIcon,
  LockIcon,
  MinusIcon,
  PanelRightIcon,
  PlusIcon,
  RefreshCwIcon,
  Undo2Icon,
} from 'lucide-vue-next'
import { PAGE_SIZE } from '../constants'
import { COLUMN_ORDERS, LAYOUT_SERVICE, SHORTCUTS, injectRequired } from '../services/keys'
import type { Session } from '../services/Session'
import type { GridColumn, Row, TablePage } from '../types'
import BottomBar from './BottomBar.vue'
import DataGrid from './DataGrid.vue'
import ExportRowsMenu from './ExportRowsMenu.vue'
import FilterBar from './FilterBar.vue'
import RowDetail from './RowDetail.vue'

// The parent shows this view only when `session.page` is set.
const props = defineProps<{ session: Session }>()
const shortcuts = injectRequired(SHORTCUTS)
const layout = injectRequired(LAYOUT_SERVICE)
const columnOrders = injectRequired(COLUMN_ORDERS)

const selected = ref<number[]>([])
const grid = useTemplateRef<InstanceType<typeof DataGrid>>('grid')
// Where the export menu is open. null = closed.
// `row` is the right-clicked row, or -1 when the menu was opened from the bottom bar.
const menu = ref<{ x: number; y: number; row: number } | null>(null)

const gridColumns = computed(toGridColumns)
// The dragged column order of this table is saved under this key.
const orderKey = computed(columnOrderKey)
const rangeText = computed(describeRange)
const pageText = computed(describePage)
const keyColumns = computed(primaryKeyNames)
const restores = computed(selectionRestores)
const exportColumns = computed(columnNames)
const exportRows = computed(rowsToExport)
const detailIndex = computed(lastSelected)
const detailRow = computed(rowOfDetail)
const detailEditable = computed(detailCanEdit)
const detailChanged = computed(changedOfDetail)
const detailTag = computed(tagOfDetail)
// The grid gets cached values. The Session getters build new arrays on every call, and a
// new array makes the grid draw all rows again. That made a drag selection slow.
const shownRows = computed(rowsOfSession)
const changedCells = computed(changedOfSession)
const deletedRows = computed(deletedOfSession)

// Row numbers mean other rows after a reload. Drop the selection.
watch(sessionPage, clearSelection)

function sessionPage(): TablePage | null {
  return props.session.page
}

function clearSelection(): void {
  selected.value = []
}

function rowsOfSession(): Row[] {
  return props.session.shownRows
}

function changedOfSession(): Record<number, number[]> {
  return props.session.changedCells
}

function deletedOfSession(): number[] {
  return props.session.deletedRows
}

function columnOrderKey(): string | undefined {
  const { config, table } = props.session
  return config && table ? columnOrders.key(config, table) : undefined
}

function toGridColumns(): GridColumn[] {
  const columns: GridColumn[] = []
  for (const column of props.session.page?.columns ?? []) {
    columns.push({ name: column.name, type: column.dataType, nullable: column.nullable })
  }
  return columns
}

/**
 * The row the detail panel shows: the last selected one. After a click that is the clicked row.
 * After a drag or a Shift-click it is the last row of the range. -1 = none.
 */
function lastSelected(): number {
  const index = selected.value.at(-1)
  return index !== undefined && index < shownRows.value.length ? index : -1
}

function rowOfDetail(): Row | null {
  return shownRows.value[detailIndex.value] ?? null
}

/** The same rule as the grid: an editable table, and not a row that is marked as deleted. */
function detailCanEdit(): boolean {
  return props.session.editable && !deletedRows.value.includes(detailIndex.value)
}

function changedOfDetail(): number[] {
  return changedCells.value[detailIndex.value] ?? []
}

function tagOfDetail(): string {
  if (deletedRows.value.includes(detailIndex.value)) {
    return 'Marked for delete'
  }
  if (detailIndex.value >= (props.session.page?.rows.length ?? 0)) {
    return 'New row'
  }
  return props.session.editable ? '' : 'Read only'
}

function columnNames(): string[] {
  const names: string[] = []
  for (const column of props.session.page?.columns ?? []) {
    names.push(column.name)
  }
  return names
}

/**
 * The selected rows, or all loaded rows when none is selected. These are the stored
 * values, without unsaved edits. Draft new rows are skipped: they are not saved yet.
 */
function rowsToExport(): Row[] {
  return props.session.pickRows(props.session.page?.rows ?? [], selected.value)
}

function openMenu(event: MouseEvent, rowIndex = -1): void {
  menu.value = { x: event.clientX, y: event.clientY, row: rowIndex }
}

/**
 * Opens the detail panel on the right-clicked row. The panel shows the last selected row,
 * so that row moves to the end of the selection. The selected rows stay the same.
 */
function showDetail(rowIndex: number): void {
  const others: number[] = []
  for (const index of selected.value) {
    if (index !== rowIndex) {
      others.push(index)
    }
  }
  selected.value = [...others, rowIndex]
  layout.openDetail()
}

function closeMenu(): void {
  menu.value = null
}

function describeRange(): string {
  const page = props.session.page
  if (!page || page.rows.length === 0) {
    return 'No rows'
  }
  const first = props.session.offset + 1
  const last = props.session.offset + page.rows.length
  return `Rows ${first.toLocaleString()}–${last.toLocaleString()} of ${page.total.toLocaleString()}`
}

function describePage(): string {
  const current = Math.floor(props.session.offset / PAGE_SIZE) + 1
  const count = Math.max(1, Math.ceil((props.session.page?.total ?? 0) / PAGE_SIZE))
  return `Page ${current.toLocaleString()} of ${count.toLocaleString()}`
}

function primaryKeyNames(): string[] {
  const names: string[] = []
  for (const column of props.session.page?.columns ?? []) {
    if (column.primaryKey) {
      names.push(column.name)
    }
  }
  return names
}

function edit(rowIndex: number, columnIndex: number, value: string | null): void {
  props.session.updateCell(rowIndex, columnIndex, value)
}

/** True when all selected rows are marked as deleted. The delete button restores them then. */
function selectionRestores(): boolean {
  return props.session.restores(selected.value)
}

/** Marks the selected rows as deleted, or restores them. Nothing is sent before Save. */
function deleteSelected(): void {
  props.session.deleteRows(selected.value)
  // The selected new rows are gone. The loaded rows stay selected, so a second click restores.
  const loaded = props.session.page?.rows.length ?? 0
  const kept: number[] = []
  for (const index of selected.value) {
    if (index < loaded) {
      kept.push(index)
    }
  }
  selected.value = kept
}
</script>

<template>
  <FilterBar :session="session" />

  <div class="body">
    <DataGrid
      ref="grid"
      v-model:selected="selected"
      :columns="gridColumns"
      :rows="shownRows"
      :editable="session.editable"
      :order-by="session.orderBy"
      :descending="session.descending"
      :new-from="session.page?.rows.length"
      :changed="changedCells"
      :deleted="deletedRows"
      :key-columns="keyColumns"
      :order-key="orderKey"
      @sort="session.run(session.sort($event))"
      @edit="edit"
      @menu="openMenu"
    />
    <RowDetail
      v-if="layout.detailOpen"
      :columns="gridColumns"
      :row="detailRow"
      :row-index="detailIndex"
      :editable="detailEditable"
      :changed="detailChanged"
      :tag="detailTag"
      :driver="session.config?.driver"
      @edit="edit"
    />
  </div>
  <ExportRowsMenu
    v-if="menu"
    :session="session"
    :x="menu.x"
    :y="menu.y"
    :columns="exportColumns"
    :rows="exportRows"
    :table="session.table"
    :copies="menu.row >= 0"
    @close="closeMenu"
    @copy="grid?.copyRows()"
  >
    <template v-if="menu.row >= 0">
      <button role="menuitem" @click="showDetail(menu.row)">
        Show row details
        <span class="hint">{{ shortcuts.hint('I') }}</span>
      </button>
      <span class="menu-line" />
    </template>
  </ExportRowsMenu>

  <BottomBar :session="session">
    <button class="icon" title="Add row" :disabled="!session.editable" @click="session.addRow()">
      <PlusIcon :size="13" />
    </button>
    <button
      class="icon"
      :title="restores ? 'Restore selected rows' : 'Delete selected rows'"
      :disabled="!session.editable || selected.length === 0"
      @click="deleteSelected"
    >
      <Undo2Icon v-if="restores" :size="13" />
      <MinusIcon v-else :size="13" />
    </button>
    <button
      class="icon"
      :title="`Reload rows (${shortcuts.hint('R')})`"
      @click="session.run(session.reload())"
    >
      <RefreshCwIcon :size="13" />
    </button>
    <button
      class="icon"
      :title="selected.length > 0 ? 'Export selected rows' : 'Export loaded rows'"
      :disabled="exportRows.length === 0"
      @click="openMenu"
    >
      <DownloadIcon :size="13" />
    </button>
    <button
      class="icon"
      :class="{ active: layout.detailOpen }"
      :title="`${layout.detailOpen ? 'Hide' : 'Show'} row details (${shortcuts.hint('I')})`"
      :aria-pressed="layout.detailOpen"
      @click="layout.toggleDetail()"
    >
      <PanelRightIcon :size="13" />
    </button>
    <span class="divider" />
    <template v-if="session.pendingCount > 0">
      <span class="pill unsaved"><span class="dot" />{{ session.unsavedText }}</span>
      <button :disabled="session.busy" @click="session.discardChanges()">Discard</button>
      <button class="primary" :disabled="session.busy" @click="session.run(session.saveChanges())">
        Save changes
        <span class="hint">{{ shortcuts.hint('S') }}</span>
      </button>
    </template>
    <span v-if="!session.editable" class="pill">
      <LockIcon :size="10" />
      Read only: {{ session.table?.kind === 'view' ? 'view' : 'no primary key' }}
    </span>
    <template #end>
      <span class="dim">{{ rangeText }}</span>
      <button
        class="icon"
        title="Previous page"
        :disabled="session.offset === 0"
        @click="session.run(session.turnPage(-1))"
      >
        <ChevronLeftIcon :size="13" />
      </button>
      <span class="page">{{ pageText }}</span>
      <button
        class="icon"
        title="Next page"
        :disabled="session.offset + PAGE_SIZE >= (session.page?.total ?? 0)"
        @click="session.run(session.turnPage(1))"
      >
        <ChevronRightIcon :size="13" />
      </button>
    </template>
  </BottomBar>
</template>

<style scoped>
/* The grid and the row detail panel, side by side. */
.body {
  display: flex;
  flex: 1;
  min-height: 0;
}

.body > .grid {
  min-width: 0;
}

/* The row details button while the panel is open. */
button.icon.active {
  color: var(--accent-text);
}

.page {
  font-weight: 500;
}
</style>
