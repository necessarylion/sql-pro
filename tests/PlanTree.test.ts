// The folding and the number text of the query plan tree. Run: bun test
import { expect, test } from 'bun:test'
import { markRaw, reactive } from 'vue'
import type { DatabaseApi } from '../src/services/DatabaseApi'
import { PlanTree } from '../src/services/PlanTree'
import { Session } from '../src/services/Session'
import { ANALYZE_STATUS, PLAN_STATUS } from '../src/constants/plan'
import type { PlanNode, QueryPlan, ResultSet } from '../src/types'
import { FakePrompt } from './FakePrompt'

function step(operation: string, children: PlanNode[] = []): PlanNode {
  return {
    operation,
    object: null,
    details: [],
    estimatedRows: null,
    actualRows: null,
    loops: null,
    cost: null,
    timeMs: null,
    share: null,
    heat: null,
    problems: [],
    children,
  }
}

const PLAN = [
  step('Sort', [step('Hash Join', [step('Seq Scan'), step('Hash', [step('Seq Scan')])])]),
]

function operations(tree: PlanTree): string[] {
  const list: string[] = []
  for (const row of tree.rows()) {
    list.push(`${row.depth}:${row.node.operation}`)
  }
  return list
}

test('the tree shows all steps from the top down, with their depth', function () {
  const tree = new PlanTree(PLAN)
  expect(operations(tree)).toEqual(['0:Sort', '1:Hash Join', '2:Seq Scan', '2:Hash', '3:Seq Scan'])
  expect(tree.rows()[3].key).toBe('0.0.1')
})

test('a closed step hides its children until it is opened again', function () {
  const tree = reactive(new PlanTree(PLAN)) as PlanTree
  tree.toggle('0.0.1')
  expect(operations(tree)).toEqual(['0:Sort', '1:Hash Join', '2:Seq Scan', '2:Hash'])
  expect(tree.rows()[3].open).toBe(false)
  tree.toggle('0')
  expect(operations(tree)).toEqual(['0:Sort'])
  tree.toggle('0')
  // The inner step stays closed.
  expect(operations(tree)).toEqual(['0:Sort', '1:Hash Join', '2:Seq Scan', '2:Hash'])
  tree.toggle('0.0.1')
  expect(operations(tree)).toHaveLength(5)
})

test('the cost column depends on the whole tree, not on what is open', function () {
  const deep = [step('Sort', [step('Seq Scan')])]
  deep[0].children[0].cost = 12
  expect(PlanTree.hasCost(deep)).toBe(true)
  expect(PlanTree.hasCost(PLAN)).toBe(false)
})

test('numbers are short and readable', function () {
  expect(PlanTree.amount(19500)).toBe('19,500')
  expect(PlanTree.amount(0.178)).toBe('0.18')
  expect(PlanTree.amount(502.26)).toBe('502')
  expect(PlanTree.amount(null)).toBe('')
  expect(PlanTree.time(0.0457)).toBe('0.046 ms')
  expect(PlanTree.time(3.466)).toBe('3.47 ms')
  expect(PlanTree.time(250.4)).toBe('250 ms')
  expect(PlanTree.time(1530)).toBe('1.53 s')
  expect(PlanTree.time(null)).toBe('')
  expect(PlanTree.share(0.3)).toBe('30%')
  expect(PlanTree.share(null)).toBe('')
})

class FakeApi {
  calls: { sql: string; analyze: boolean }[] = []

  async queryPlan(_id: string, sql: string, analyze: boolean): Promise<QueryPlan> {
    this.calls.push({ sql, analyze })
    const raw: ResultSet = {
      columns: ['QUERY PLAN'],
      rows: [[`plan of ${sql}`]],
      affected: 0,
      truncated: false,
    }
    return { nodes: PLAN, analyzed: analyze, measure: analyze ? 'time' : 'cost', total: 1, raw }
  }

  async runQuery(): Promise<ResultSet[]> {
    return [{ columns: ['id'], rows: [['1']], affected: 0, truncated: false }]
  }

  async listTables(): Promise<[]> {
    return []
  }
}

function session(api: FakeApi): Session {
  return reactive(new Session(markRaw(api) as unknown as DatabaseApi, new FakePrompt())) as Session
}

test('explain shows the tree, and the plain EXPLAIN as the result', async function () {
  const api = new FakeApi()
  const tab = session(api)
  await tab.explain('DELETE FROM users')
  expect(api.calls).toEqual([{ sql: 'DELETE FROM users', analyze: false }])
  expect(tab.plan?.nodes).toHaveLength(1)
  expect(tab.result?.rows).toEqual([['plan of DELETE FROM users']])
  expect(tab.status).toBe(PLAN_STATUS)
})

test('analyze asks for real times, and a normal run hides the plan again', async function () {
  const api = new FakeApi()
  const tab = session(api)
  await tab.explain('SELECT 1', true)
  expect(api.calls).toEqual([{ sql: 'SELECT 1', analyze: true }])
  expect(tab.plan?.analyzed).toBe(true)
  expect(tab.status).toBe(ANALYZE_STATUS)
  expect(tab.cancellable).toBe(false)
  await tab.runQuery('SELECT 1')
  expect(tab.plan).toBeNull()
  expect(tab.result?.columns).toEqual(['id'])
})
