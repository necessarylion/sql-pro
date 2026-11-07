// Checks the Data mode of the Compare view with a fake backend. Run: bun test
import { expect, test } from 'bun:test'
import { DEFAULT_CONFIGS } from '../src/constants'
import type { DatabaseApi } from '../src/services/DatabaseApi'
import { DataSync } from '../src/services/DataSync'
import { Session } from '../src/services/Session'
import { FakePrompt } from './FakePrompt'
import type { ConnectionConfig, DataDiff, DataKinds, TableRef, UserList } from '../src/types'

function table(name: string, kind = 'table'): TableRef {
  return { schema: null, name, kind }
}

function diff(): DataDiff {
  return {
    key: ['id'],
    columns: ['id', 'name'],
    sourceOnly: [],
    targetOnly: ['old'],
    generated: [],
    limited: false,
    deleteRows: 1,
    updateRows: 2,
    insertRows: 1,
    deletes: ['DELETE 5'],
    updates: ['UPDATE 2', 'UPDATE 3'],
    inserts: ['INSERT 4'],
    database: 'shop',
  }
}

class FakeApi {
  compares: unknown[][] = []
  runs: unknown[][] = []
  result: DataDiff = diff()
  tables: TableRef[] = [table('users'), table('orders'), table('report', 'view')]

  async connect(): Promise<void> {}

  async useDatabase(): Promise<void> {}

  async listDatabases(): Promise<string[]> {
    return ['shop']
  }

  async listTables(): Promise<TableRef[]> {
    return this.tables
  }

  async listObjects(): Promise<[]> {
    return []
  }

  async listUsers(): Promise<UserList> {
    return { canManage: false, users: [] }
  }

  async compareData(source: string, target: string, picked: TableRef): Promise<DataDiff> {
    this.compares.push([source, target, picked.name])
    return this.result
  }

  async runDataSync(
    source: string,
    target: string,
    picked: TableRef,
    kinds: DataKinds,
    reviewed: string[],
    database: string | null,
  ): Promise<void> {
    this.runs.push([source, target, picked.name, kinds, reviewed, database])
  }
}

async function tab(api: FakeApi, config: ConnectionConfig, title: string): Promise<Session> {
  const session = new Session(api as unknown as DatabaseApi, new FakePrompt())
  await session.connect(config, title)
  return session
}

function setup(): { api: FakeApi; prompt: FakePrompt; sync: DataSync } {
  const api = new FakeApi()
  const prompt = new FakePrompt()
  return { api, prompt, sync: new DataSync(api as unknown as DatabaseApi, prompt) }
}

test('only tables that both tabs have can be picked, no views', async function () {
  const { api } = setup()
  const dev = await tab(api, DEFAULT_CONFIGS.mysql, 'dev')
  api.tables = [table('users'), table('report', 'view')]
  const prod = await tab(api, DEFAULT_CONFIGS.mysql, 'prod')
  expect(DataSync.tables(dev, prod)).toEqual([table('users')])
})

test('compare needs a table and gives the SQL of the ticked kinds in order', async function () {
  const { api, sync } = setup()
  const dev = await tab(api, DEFAULT_CONFIGS.mysql, 'dev')
  const prod = await tab(api, DEFAULT_CONFIGS.mysql, 'prod')
  await expect(sync.compare(dev, prod)).rejects.toContain('Pick a table')
  sync.pickTable('users')
  await sync.compare(dev, prod)
  expect(api.compares).toEqual([[dev.id, prod.id, 'users']])
  expect(sync.statements).toEqual(['DELETE 5', 'UPDATE 2', 'UPDATE 3', 'INSERT 4'])
  sync.toggle('delete')
  expect(sync.statements).toEqual(['UPDATE 2', 'UPDATE 3', 'INSERT 4'])
  expect(sync.script).toBe('UPDATE 2;\nUPDATE 3;\nINSERT 4;\n')
})

test('too many differences give no SQL', async function () {
  const { api, sync } = setup()
  api.result = { ...diff(), limited: true, deletes: [], updates: [], inserts: [] }
  const dev = await tab(api, DEFAULT_CONFIGS.mysql, 'dev')
  const prod = await tab(api, DEFAULT_CONFIGS.mysql, 'prod')
  sync.pickTable('users')
  await sync.compare(dev, prod)
  expect(sync.script).toBe('')
  await expect(sync.runOnTarget(dev, prod)).rejects.toContain('no SQL')
  expect(api.runs).toEqual([])
})

test('run asks first with the row counts, and does nothing on no', async function () {
  const { api, prompt, sync } = setup()
  const dev = await tab(api, DEFAULT_CONFIGS.mysql, 'dev')
  const prod = await tab(api, DEFAULT_CONFIGS.mysql, 'prod')
  sync.pickTable('users')
  await sync.compare(dev, prod)
  sync.toggle('insert')
  prompt.answer = false
  expect(await sync.runOnTarget(dev, prod)).toBe(false)
  expect(api.runs).toEqual([])
  expect(prompt.questions[0]).toContain('"prod" (users)')
  expect(prompt.questions[0]).toContain('It deletes 1 row, updates 2 rows. This cannot be undone.')

  prompt.answer = true
  expect(await sync.runOnTarget(dev, prod)).toBe(true)
  const kinds = { insert: false, update: true, delete: true }
  expect(api.runs).toEqual([
    [dev.id, prod.id, 'users', kinds, ['DELETE 5', 'UPDATE 2', 'UPDATE 3'], 'shop'],
  ])
  // It compares again after the run.
  expect(api.compares.length).toBe(2)
})

test('a new tab, table or database drops the result, so it cannot run there', async function () {
  const { api, sync } = setup()
  const dev = await tab(api, DEFAULT_CONFIGS.mysql, 'dev')
  const prod = await tab(api, DEFAULT_CONFIGS.mysql, 'prod')
  sync.pickTable('users')
  await sync.compare(dev, prod)
  sync.pickTable('orders')
  expect(sync.diff).toBeNull()
  expect(sync.script).toBe('')

  sync.pickTable('users')
  await sync.compare(dev, prod)
  expect(sync.fits(dev, prod)).toBe(true)
  await prod.useDatabase('other')
  expect(sync.fits(dev, prod)).toBe(false)
  await expect(sync.runOnTarget(dev, prod)).rejects.toContain('Compare again')
  let refused = ''
  try {
    sync.openInQuery(dev, prod)
  } catch (error) {
    refused = String(error)
  }
  expect(refused).toContain('Compare again')
  expect(prod.sql).toBe('')
  expect(api.runs).toEqual([])

  // A switch of the tab that holds the Compare view forgets it too.
  dev.dataSync.pickTable('users')
  await dev.dataSync.compare(dev, prod)
  expect(dev.dataSync.diff).not.toBeNull()
  await dev.useDatabase('other')
  expect(dev.dataSync.diff).toBeNull()
})

test('open in Query puts the SQL into the target tab', async function () {
  const { api, sync } = setup()
  const dev = await tab(api, DEFAULT_CONFIGS.sqlite, 'dev')
  const prod = await tab(api, DEFAULT_CONFIGS.sqlite, 'prod')
  sync.pickTable('users')
  await sync.compare(dev, prod)
  sync.openInQuery(dev, prod)
  expect(prod.view).toBe('query')
  expect(prod.sql).toBe(sync.script)
})
