<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { confirm } from '@tauri-apps/plugin-dialog'
import {
  ChevronLeftIcon,
  ChevronRightIcon,
  LockIcon,
  MinusIcon,
  PlusIcon,
  RefreshCwIcon,
} from 'lucide-vue-next'
import { PAGE_SIZE } from '../constants'
import type { Session } from '../services/Session'
import type { GridColumn, Row, TablePage } from '../types'
import BottomBar from './BottomBar.vue'
import DataGrid from './DataGrid.vue'
import FilterBar from './FilterBar.vue'

// The parent shows this view only when `session.page` is set.
const props = defineProps<{ session: Session }>()

const selected = ref<number[]>([])

const gridColumns = computed(toGridColumns)
const gridRows = computed(rowsWithDraft)
const draftIndex = computed(indexOfDraft)
const rangeText = computed(describeRange)
const pageText = computed(describePage)
const keyColumns = computed(primaryKeyNames)

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

function rowsWithDraft(): Row[] {
  const rows = props.session.page?.rows ?? []
  return props.session.draft ? [...rows, props.session.draft] : rows
}

function indexOfDraft(): number | undefined {
  return props.session.draft ? props.session.page?.rows.length : undefined
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
  props.session.run(props.session.updateCell(rowIndex, columnIndex, value))
}

async function deleteSelected(): Promise<void> {
  const count = selected.value.length
  const sure = await confirm(`Delete ${count} selected ${count === 1 ? 'row' : 'rows'}?`, {
    title: 'Delete rows',
    kind: 'warning',
  })
  if (sure) {
    await props.session.deleteRows(selected.value)
    selected.value = []
  }
}
</script>

<template>
  <FilterBar :session="session" />

  <DataGrid
    v-model:selected="selected"
    :columns="gridColumns"
    :rows="gridRows"
    :editable="session.editable"
    :order-by="session.orderBy"
    :descending="session.descending"
    :draft-index="draftIndex"
    :key-columns="keyColumns"
    gutter
    @sort="session.run(session.sort($event))"
    @edit="edit"
  />

  <BottomBar :session="session">
    <button
      class="icon"
      title="Add row"
      :disabled="!session.editable || !!session.draft"
      @click="session.addDraft()"
    >
      <PlusIcon :size="13" />
    </button>
    <button
      class="icon"
      title="Delete selected rows"
      :disabled="!session.editable || selected.length === 0"
      @click="session.run(deleteSelected())"
    >
      <MinusIcon :size="13" />
    </button>
    <button class="icon" title="Reload rows" @click="session.run(session.loadPage())">
      <RefreshCwIcon :size="13" />
    </button>
    <span class="divider" />
    <template v-if="session.draft">
      <span class="pill unsaved"><span class="dot" />1 unsaved row</span>
      <button @click="session.discardDraft()">Discard</button>
      <button class="primary" @click="session.run(session.saveDraft())">Save row</button>
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
