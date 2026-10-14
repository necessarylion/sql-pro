// Checks "Open in new tab" and closing a tab, with a fake backend. Run: bun test
import { expect, test } from 'bun:test'
import type { DatabaseApi } from '../src/services/DatabaseApi'
import type { Session } from '../src/services/Session'
import { SessionManager } from '../src/services/SessionManager'
import type { ConnectionConfig, TablePage, TableRef, UserList } from '../src/types'
import { FakePrompt } from './FakePrompt'

const ORDERS: TableRef = { schema: null, name: 'orders', kind: 'table' }
const CONFIG: ConnectionConfig = {
  driver: 'mysql',
  host: 'db',
  port: 3306,
  user: 'root',
  password: 'secret',
  database: 'shop',
  ssh: null,
}

class FakeApi {
  clones: { id: string; newId: string }[] = []
  disconnected: string[] = []
  failClone = false

  async connect(): Promise<void> {}

  async cloneConnection(id: string, newId: string): Promise<void> {
    if (this.failClone) {
      throw 'Not connected'
    }
    this.clones.push({ id, newId })
  }

  async disconnect(id: string): Promise<void> {
    this.disconnected.push(id)
  }

  async listDatabases(): Promise<string[]> {
    return ['shop']
  }

  async listTables(): Promise<TableRef[]> {
    return [ORDERS]
  }

  async listUsers(): Promise<UserList> {
    return { canManage: false, users: [] }
  }

  async browse(): Promise<TablePage> {
    return {
      columns: [
        {
          name: 'id',
          dataType: 'int',
          nullable: false,
          default: null,
          primaryKey: true,
          extra: '',
        },
      ],
      rows: [['1']],
      total: 1,
    }
  }
}

async function connected(): Promise<{
  api: FakeApi
  manager: SessionManager
  first: Session
  prompt: FakePrompt
}> {
  const api = new FakeApi()
  const prompt = new FakePrompt()
  const manager = new SessionManager(api as unknown as DatabaseApi, prompt)
  const first = manager.sessions[0]
  await first.connect(CONFIG, 'Shop', 'prod')
  return { api, manager, first, prompt }
}

test('a table opens in a new active tab with a copy of the connection', async function () {
  const { api, manager, first } = await connected()
  expect(await first.run(manager.openTable(first, ORDERS))).toBe(true)

  expect(manager.sessions.length).toBe(2)
  const tab = manager.sessions[1]
  expect(api.clones).toEqual([{ id: first.id, newId: tab.id }])
  expect(manager.activeId).toBe(tab.id)
  expect(manager.active).toBe(tab)
  expect(tab.title).toBe('Shop')
  expect(tab.environment).toBe('prod')
  expect(tab.config?.database).toBe('shop')
  expect(tab.config?.password).toBe('')
  expect(tab.tables).toEqual([ORDERS])
  expect(tab.table).toBe(ORDERS)
  expect(tab.view).toBe('content')
  expect(tab.page?.rows).toEqual([['1']])
  // The first tab is not touched.
  expect(first.table).toBeNull()
})

test('a failing copy adds no tab and shows the error in the old tab', async function () {
  const { api, manager, first } = await connected()
  api.failClone = true
  expect(await first.run(manager.openTable(first, ORDERS))).toBe(false)
  expect(first.error).toBe('Not connected')
  expect(manager.sessions).toEqual([first])
  expect(manager.activeId).toBe(first.id)
})

test('closing a tab with unsaved changes asks first', async function () {
  const { api, manager, first, prompt } = await connected()
  await first.selectTable(ORDERS)
  first.addRow()

  prompt.answer = false
  await manager.close(first)
  expect(manager.sessions).toEqual([first])
  expect(api.disconnected).toEqual([])

  prompt.answer = true
  await manager.close(first)
  expect(prompt.questions.length).toBe(2)
  expect(api.disconnected).toEqual([first.id])
  expect(manager.sessions.length).toBe(1)
  expect(manager.sessions[0]).not.toBe(first)
})
