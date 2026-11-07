<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue'
import { ChevronDownIcon, ChevronRightIcon } from 'lucide-vue-next'
import {
  PLAN_COLUMN_GAP,
  PLAN_NUMBER_WIDTH,
  PLAN_PROBLEM_HINTS,
  PLAN_PROBLEM_LABELS,
  PLAN_STEP_MIN,
} from '../constants/plan'
import { PlanTree } from '../services/PlanTree'
import type { PlanRow } from '../services/PlanTree'
import type { QueryPlan } from '../types'

const props = defineProps<{ plan: QueryPlan }>()
const tree = computed(buildTree)
const rows = computed(visibleRows)
const hasCost = computed(anyCost)
// The step whose details show under it. null = none.
const picked = ref<string | null>(null)
const columns = computed(gridColumns)

// A new plan has other steps. Its details start closed.
watch(currentPlan, clearPick)

function currentPlan(): QueryPlan {
  return props.plan
}

function buildTree(): PlanTree {
  return reactive(new PlanTree(props.plan.nodes)) as PlanTree
}

function visibleRows(): PlanRow[] {
  return tree.value.rows()
}

function anyCost(): boolean {
  return rows.value.some(hasCostValue)
}

function hasCostValue(row: PlanRow): boolean {
  return row.node.cost !== null
}

function clearPick(): void {
  picked.value = null
}

/**
 * Only the columns this plan has numbers for. Analyze adds the real rows, loops and time.
 * All rows use the same widths, so the columns line up. A long name is cut, not the numbers.
 */
function gridColumns(): { '--plan-columns': string; '--plan-min': string } {
  const widths = [PLAN_STEP_MIN, PLAN_NUMBER_WIDTH]
  if (props.plan.analyzed) {
    widths.push(PLAN_NUMBER_WIDTH, PLAN_NUMBER_WIDTH)
  }
  if (hasCost.value) {
    widths.push(PLAN_NUMBER_WIDTH)
  }
  if (props.plan.analyzed) {
    widths.push(PLAN_NUMBER_WIDTH)
  }
  if (props.plan.measure) {
    widths.push(PLAN_NUMBER_WIDTH)
  }
  const tracks = widths.map(toPixels)
  tracks[0] = `minmax(${tracks[0]}, 1fr)`
  // The gaps between the columns, the heat bar and the right padding.
  const room = PLAN_COLUMN_GAP * (widths.length - 1) + 3 + 10
  return {
    '--plan-columns': tracks.join(' '),
    '--plan-min': toPixels(sum(widths) + room),
  }
}

function toPixels(width: number): string {
  return `${width}px`
}

function sum(widths: number[]): number {
  let total = 0
  for (const width of widths) {
    total += width
  }
  return total
}

function pick(row: PlanRow): void {
  picked.value = picked.value === row.key || row.node.details.length === 0 ? null : row.key
}

function indent(row: PlanRow): string {
  return `calc(${row.depth * 14}px + 4px)`
}

function shareWidth(row: PlanRow): string {
  return `${Math.round((row.node.share ?? 0) * 100)}%`
}

function shareTitle(): string {
  const what = props.plan.measure === 'time' ? 'real time' : 'estimated cost'
  return `The part of the whole ${what} this step takes itself, without the steps below it`
}
</script>

<template>
  <div class="plan" :style="columns" role="tree">
    <div class="row head">
      <span>Step</span>
      <span class="num" title="The rows the planner expects, for one loop">Est. rows</span>
      <template v-if="plan.analyzed">
        <span class="num" title="The real rows, for one loop">Rows</span>
        <span class="num" title="How often the step ran">Loops</span>
      </template>
      <span v-if="hasCost" class="num" title="The estimated cost, with the steps below">Cost</span>
      <span v-if="plan.analyzed" class="num" title="The real time, with the steps below, all loops">
        Time
      </span>
      <span v-if="plan.measure" class="num" :title="shareTitle()">Share</span>
    </div>
    <template v-for="row in rows" :key="row.key">
      <div
        class="row step"
        :class="[row.node.heat, { picked: picked === row.key, more: row.node.details.length > 0 }]"
        role="treeitem"
        :aria-expanded="row.node.children.length > 0 ? row.open : undefined"
        :aria-level="row.depth + 1"
        @click="pick(row)"
      >
        <span class="name" :style="{ paddingLeft: indent(row) }">
          <button
            v-if="row.node.children.length > 0"
            class="icon fold"
            :title="row.open ? 'Hide the steps below' : 'Show the steps below'"
            @click.stop="tree.toggle(row.key)"
          >
            <component :is="row.open ? ChevronDownIcon : ChevronRightIcon" :size="12" />
          </button>
          <span v-else class="fold" />
          <span class="operation">{{ row.node.operation }}</span>
          <span v-if="row.node.object" class="object" :title="row.node.object">
            {{ row.node.object }}
          </span>
          <span
            v-for="problem in row.node.problems"
            :key="problem"
            class="badge"
            :class="problem"
            :title="PLAN_PROBLEM_HINTS[problem]"
          >
            {{ PLAN_PROBLEM_LABELS[problem] }}
          </span>
        </span>
        <span class="num">{{ PlanTree.amount(row.node.estimatedRows) }}</span>
        <template v-if="plan.analyzed">
          <span class="num">{{ PlanTree.amount(row.node.actualRows) }}</span>
          <span class="num">{{ PlanTree.amount(row.node.loops) }}</span>
        </template>
        <span v-if="hasCost" class="num">{{ PlanTree.amount(row.node.cost) }}</span>
        <span v-if="plan.analyzed" class="num">{{ PlanTree.time(row.node.timeMs) }}</span>
        <span v-if="plan.measure" class="share">
          <span class="bar-fill" :style="{ width: shareWidth(row) }" />
          <span class="num">{{ PlanTree.share(row.node.share) }}</span>
        </span>
      </div>
      <dl v-if="picked === row.key" class="details" :style="{ paddingLeft: indent(row) }">
        <template v-for="detail in row.node.details" :key="detail.name">
          <dt>{{ detail.name }}</dt>
          <dd>{{ detail.value }}</dd>
        </template>
      </dl>
    </template>
  </div>
</template>

<style scoped>
.plan {
  flex: 1;
  min-width: 0;
  overflow: auto;
  font-size: calc(11px * var(--font-scale));
}

.row {
  display: grid;
  grid-template-columns: var(--plan-columns);
  gap: 8px;
  align-items: center;
  min-width: var(--plan-min);
  height: 24px;
  padding-right: 10px;
  border-bottom: 1px solid var(--border-soft);
  /* The heat colour shows as a bar on the left edge. */
  border-left: 3px solid transparent;
  white-space: nowrap;
}

.head {
  position: sticky;
  top: 0;
  z-index: 1;
  height: 28px;
  background: var(--header);
  border-bottom-color: var(--border);
  color: var(--text-2);
  font-weight: 600;
}

.head > span:first-child {
  padding-left: 26px;
}

.step.more {
  cursor: pointer;
}

.step:hover {
  background: var(--tab-hover);
}

.step.picked {
  background: var(--picked);
}

.step.slow {
  border-left-color: var(--danger);
  background: color-mix(in srgb, var(--danger) 9%, transparent);
}

.step.warm {
  border-left-color: var(--warn);
  background: color-mix(in srgb, var(--warn) 8%, transparent);
}

.step.slow.picked,
.step.warm.picked {
  background: var(--picked);
}

.name {
  display: flex;
  gap: 6px;
  align-items: center;
  min-width: 0;
  overflow: hidden;
}

.fold {
  flex: none;
  width: 16px;
  height: 16px;
  color: var(--text-3);
}

.operation {
  flex: none;
  font-weight: 600;
}

.object {
  min-width: 0;
  overflow: hidden;
  color: var(--text-2);
  font-family: var(--font-mono);
  text-overflow: ellipsis;
}

.badge {
  flex: none;
  height: 15px;
  padding: 0 5px;
  border-radius: 3px;
  font-size: calc(9px * var(--font-scale));
  font-weight: 700;
  line-height: 15px;
  background: var(--modified);
  color: var(--warn);
}

.badge.fullScan {
  background: var(--deleted);
  color: var(--danger);
}

.badge.rowEstimate {
  background: var(--stmt-highlight);
  color: var(--accent-text);
}

.num {
  text-align: right;
  font-variant-numeric: tabular-nums;
}

.slow .share .num {
  color: var(--danger);
  font-weight: 600;
}

.warm .share .num {
  color: var(--warn);
  font-weight: 600;
}

/* A small bar under the share number shows the part at a glance. */
.share {
  position: relative;
  display: flex;
  justify-content: flex-end;
  height: 16px;
  align-items: center;
}

.share .num {
  position: relative;
}

.bar-fill {
  position: absolute;
  left: 0;
  bottom: 1px;
  height: 2px;
  max-width: 100%;
  background: var(--text-3);
  border-radius: 1px;
}

.slow .bar-fill {
  background: var(--danger);
}

.warm .bar-fill {
  background: var(--warn);
}

/* The facts of the picked step, like its filter. They can be selected and copied. */
.details {
  display: grid;
  grid-template-columns: max-content minmax(0, 1fr);
  gap: 2px 10px;
  min-width: var(--plan-min);
  margin: 0;
  padding: 6px 10px 8px;
  border-bottom: 1px solid var(--border-soft);
  background: var(--row-alt);
  -webkit-user-select: text;
  user-select: text;
}

.details dt {
  margin-left: 26px;
  color: var(--text-2);
}

.details dd {
  margin: 0;
  font-family: var(--font-mono);
  white-space: pre-wrap;
  word-break: break-word;
}
</style>
