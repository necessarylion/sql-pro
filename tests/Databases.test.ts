// Checks how the Session creates and drops databases, with a fake backend. Run: bun test
import { expect, test } from 'bun:test'
import { DEFAULT_CONFIGS } from '../src/constants'
import type { DatabaseApi } from '../src/services/DatabaseApi'
import { Session } from '../src/services/Session'
import { FakePrompt } from './FakePrompt'
import type { ConnectionConfig, TableRef, UserList } from '../src/types'

/** A server with a list of databases. It notes the calls that change something. */
class FakeApi {
  databases = ['postgres', 'shop']
  calls: string[][] = []

  async connect(): Promise<void> {}

  async useDatabase(_id: string, name: string): Promise<void> {
    this.calls.push(['use', name])
  }

  async listDatabases(): Promise<string[]> {
    return [...this.databases]
  }

  async createDatabase(
    _id: string,
    name: string,
    charset: string,
    collation: string,
  ): Promise<void> {
    this.calls.push(['create', name, charset, collation])
    this.databases.push(name)
  }

  async dropDatabase(_id: string, name: string): Promise<void> {
    if (!this.databases.includes(name)) {
      throw `Unknown database: ${name}`
    }
    this.calls.push(['drop', name])
    this.databases = this.databases.filter(function (other) {
      return other !== name
    })
  }

  async listTables(): Promise<TableRef[]> {
    return []
  }

  async listObjects(): Promise<[]> {
    return []
  }

  async listUsers(): Promise<UserList> {
    return { canManage: false, users: [] }
  }
}

async function connected(api: FakeApi, config: ConnectionConfig): Promise<Session> {
  const session = new Session(api as unknown as DatabaseApi, new FakePrompt())
  await session.connect({ ...config, database: 'shop' }, 'test')
  return session
}

test('create sends the trimmed names, loads the list and switches to the new database', async function () {
  const api = new FakeApi()
  const session = await connected(api, DEFAULT_CONFIGS.mysql)
  await session.createDatabase(' sales ', ' utf8mb4 ', '')
  expect(api.calls).toEqual([
    ['create', 'sales', 'utf8mb4', ''],
    ['use', 'sales'],
  ])
  expect(session.databases).toContain('sales')
  expect(session.config?.database).toBe('sales')
  expect(session.status).toBe('Database "sales" created')
})

test('drop of another database keeps the current one', async function () {
  const api = new FakeApi()
  api.databases.push('old')
  const session = await connected(api, DEFAULT_CONFIGS.postgres)
  await session.dropDatabase('old')
  expect(api.calls).toEqual([['drop', 'old']])
  expect(session.databases).toEqual(['postgres', 'shop'])
  expect(session.config?.database).toBe('shop')
})

test('drop of the current database switches to a system database first', async function () {
  const api = new FakeApi()
  const session = await connected(api, DEFAULT_CONFIGS.postgres)
  await session.dropDatabase('shop')
  expect(api.calls).toEqual([
    ['use', 'postgres'],
    ['drop', 'shop'],
  ])
  expect(session.databases).toEqual(['postgres'])
  expect(session.config?.database).toBe('postgres')
})

test('MySQL leaves the dropped database for no database at all', async function () {
  const api = new FakeApi()
  const session = await connected(api, DEFAULT_CONFIGS.mysql)
  await session.dropDatabase('shop')
  expect(api.calls).toEqual([
    ['use', ''],
    ['drop', 'shop'],
  ])
  expect(session.config?.database).toBe('')
})

test('a failed drop still loads the list again', async function () {
  const api = new FakeApi()
  const session = await connected(api, DEFAULT_CONFIGS.postgres)
  api.databases.push('late')
  await expect(session.dropDatabase('missing')).rejects.toBe('Unknown database: missing')
  expect(session.databases).toContain('late')
})

test('SQLite cannot change databases', async function () {
  const session = await connected(new FakeApi(), DEFAULT_CONFIGS.sqlite)
  expect(session.canChangeDatabases).toBe(false)
  const server = await connected(new FakeApi(), DEFAULT_CONFIGS.mssql)
  expect(server.canChangeDatabases).toBe(true)
})
