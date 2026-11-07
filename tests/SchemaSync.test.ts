// Checks the Compare view logic with a fake backend. Run: bun test
import { expect, test } from 'bun:test'
import { DEFAULT_CONFIGS } from '../src/constants'
import type { DatabaseApi } from '../src/services/DatabaseApi'
import { SchemaSync } from '../src/services/SchemaSync'
import { Session } from '../src/services/Session'
import { FakePrompt } from './FakePrompt'
import type { ConnectionConfig, SchemaChange, SyncSql, TableRef, UserList } from '../src/types'

function change(table: string, action: SchemaChange['action'], name = ''): SchemaChange {
  return {
    id: `${action}:${table}:${name}`,
    table: { schema: null, name: table, kind: 'table' },
    action,
    name,
    detail: '',
  }
}

const CHANGES: SchemaChange[] = [
  change('old', 'dropTable'),
  change('posts', 'createTable'),
  change('users', 'addColumn', 'email'),
  change('users', 'dropColumn', 'age'),
]

class FakeApi {
  compares: string[][] = []
  picks: string[][] = []
  runs: unknown[][] = []
  changes = CHANGES
  tableLoads = 0

  async connect(): Promise<void> {}

  async useDatabase(): Promise<void> {}

  async listDatabases(): Promise<string[]> {
    return ['shop']
  }

  async listTables(): Promise<TableRef[]> {
    this.tableLoads += 1
    return []
  }

  async listObjects(): Promise<[]> {
    return []
  }

  async listUsers(): Promise<UserList> {
    return { canManage: false, users: [] }
  }

  async compareSchemas(source: string, target: string): Promise<SchemaChange[]> {
    this.compares.push([source, target])
    return this.changes
  }

  async schemaSyncSql(_source: string, _target: string, picked: string[]): Promise<SyncSql> {
    this.picks.push([...picked])
    const statements: string[] = []
    for (const id of picked) {
      statements.push(`-- ${id}`)
    }
    return { statements, warnings: [] }
  }

  async runSchemaSync(target: string, statements: string[]): Promise<void> {
    this.runs.push([target, statements])
  }
}

async function tab(api: FakeApi, config: ConnectionConfig, title: string): Promise<Session> {
  const session = new Session(api as unknown as DatabaseApi, new FakePrompt())
  await session.connect(config, title)
  return session
}

function setup(): { api: FakeApi; prompt: FakePrompt; sync: SchemaSync } {
  const api = new FakeApi()
  const prompt = new FakePrompt()
  return { api, prompt, sync: new SchemaSync(api as unknown as DatabaseApi, prompt) }
}

test('two tabs of different kinds or one tab cannot be compared', async function () {
  const api = new FakeApi()
  const mysql = await tab(api, DEFAULT_CONFIGS.mysql, 'dev')
  const postgres = await tab(api, DEFAULT_CONFIGS.postgres, 'prod')
  expect(SchemaSync.problem(mysql, postgres)).toBe(
    'Both tabs must use the same kind of database. The source is MySQL and the target is PostgreSQL.',
  )
  expect(SchemaSync.problem(mysql, mysql)).toBe('Pick two different tabs.')
  expect(SchemaSync.problem(mysql, undefined)).toContain('Both must be connected')
  const { sync } = setup()
  await expect(sync.compare(mysql, postgres)).rejects.toContain('same kind')
  expect(api.compares).toEqual([])
})

test('compare picks every difference and makes the SQL at once', async function () {
  const { api, sync } = setup()
  const dev = await tab(api, DEFAULT_CONFIGS.mysql, 'dev')
  const prod = await tab(api, DEFAULT_CONFIGS.mysql, 'prod')
  await sync.compare(dev, prod)
  expect(api.compares).toEqual([[dev.id, prod.id]])
  expect(sync.picked.length).toBe(4)
  expect(sync.script).toContain('-- addColumn:users:email;')

  const groups = sync.groups()
  expect(groups.map(titleOf)).toEqual(['Only in source', 'Only in target', 'Changed tables'])
  expect(groups[2].tables).toEqual([{ label: 'users', changes: [CHANGES[2], CHANGES[3]] }])
})

function titleOf(group: { title: string }): string {
  return group.title
}

test('a new pick drops the old SQL until it is made again', async function () {
  const { api, sync } = setup()
  const dev = await tab(api, DEFAULT_CONFIGS.mysql, 'dev')
  const prod = await tab(api, DEFAULT_CONFIGS.mysql, 'prod')
  await sync.compare(dev, prod)
  sync.toggle(CHANGES[0])
  expect(sync.sql).toBeNull()
  expect(sync.script).toBe('')
  await sync.makeSql()
  expect(api.picks.at(-1)).not.toContain(CHANGES[0].id)

  const users = [CHANGES[2], CHANGES[3]]
  sync.toggleAll(users)
  expect(sync.allPicked(users)).toBe(false)
  sync.toggle(CHANGES[2])
  sync.toggleAll(users)
  expect(sync.allPicked(users)).toBe(true)
})

test('run asks first, names the drops, and does nothing on no', async function () {
  const { api, prompt, sync } = setup()
  const dev = await tab(api, DEFAULT_CONFIGS.mysql, 'dev')
  const prod = await tab(api, DEFAULT_CONFIGS.mysql, 'prod')
  await sync.compare(dev, prod)
  prompt.answer = false
  expect(await sync.runOnTarget(dev, prod)).toBe(false)
  expect(api.runs).toEqual([])
  expect(prompt.questions[0]).toContain('Run 4 statements on "prod"?')
  expect(prompt.questions[0]).toContain('It drops 1 table and 1 column')
  expect(prompt.questions[0]).toContain('MySQL cannot undo')

  prompt.answer = true
  const loads = api.tableLoads
  api.changes = []
  expect(await sync.runOnTarget(dev, prod)).toBe(true)
  expect(api.runs[0][0]).toBe(prod.id)
  expect((api.runs[0][1] as string[]).length).toBe(4)
  // The target lists its tables again, and the compare runs again.
  expect(api.tableLoads).toBe(loads + 1)
  expect(sync.changes).toEqual([])
  expect(sync.script).toBe('')
})

test('open in Query puts the SQL into the target tab', async function () {
  const { api, sync } = setup()
  const dev = await tab(api, DEFAULT_CONFIGS.sqlite, 'dev')
  const prod = await tab(api, DEFAULT_CONFIGS.sqlite, 'prod')
  await sync.compare(dev, prod)
  sync.openInQuery(prod)
  expect(prod.view).toBe('query')
  expect(prod.sql).toBe(sync.script)
  expect(dev.sql).toBe('')
})

test('a switch to another database forgets the differences', async function () {
  const api = new FakeApi()
  const dev = await tab(api, DEFAULT_CONFIGS.mysql, 'dev')
  const prod = await tab(api, DEFAULT_CONFIGS.mysql, 'prod')
  await dev.schemaSync.compare(dev, prod)
  expect(dev.schemaSync.changes).toEqual(CHANGES)
  await dev.useDatabase('other')
  expect(dev.schemaSync.changes).toBeNull()
  expect(dev.schemaSync.picked).toEqual([])
})
