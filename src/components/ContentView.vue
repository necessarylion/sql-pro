<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import {
  ChevronLeftIcon,
  ChevronRightIcon,
  LockIcon,
  MinusIcon,
  PlusIcon,
  RefreshCwIcon,
  Undo2Icon,
} from 'lucide-vue-next'
import { PAGE_SIZE } from '../constants'
import { SHORTCUTS, injectRequired } from '../services/keys'
import type { Session } from '../services/Session'
import type { GridColumn, TablePage } from '../types'
import BottomBar from './BottomBar.vue'
import DataGrid from './DataGrid.vue'
import FilterBar from './FilterBar.vue'

// The parent shows this view only when `session.page` is set.
const props = defineProps<{ session: Session }>()
const shortcuts = injectRequired(SHORTCUTS)

const selected = ref<number[]>([])

const gridColumns = computed(toGridColumns)
const rangeText = computed(describeRange)
const pageText = computed(describePage)
const keyColumns = computed(primaryKeyNames)
const restores = computed(selectionRestores)

// Row numbers mean other rows after a reload. Drop the selection.
watch(sessionPage, clearSelection)

function sessionPage(): TablePage | null {
  return props.session.page
}

function clearSelection(): void {
  selected.value = []
}

function toGridColumns(): GridColumn[] {
  const columns: GridColumn[] = []
  for (const column of props.session.page?.columns ?? []) {
    columns.push({ name: column.name, type: column.dataType, nullable: column.nullable })
  }
  return columns
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

  <DataGrid
    v-model:selected="selected"
    :columns="gridColumns"
    :rows="session.shownRows"
    :editable="session.editable"
    :order-by="session.orderBy"
    :descending="session.descending"
    :new-from="session.page?.rows.length"
    :changed="session.changedCells"
    :deleted="session.deletedRows"
    :key-columns="keyColumns"
    @sort="session.run(session.sort($event))"
    @edit="edit"
  />

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
.page {
  font-weight: 500;
}
</style>
