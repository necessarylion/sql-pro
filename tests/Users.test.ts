// Checks the user methods of the Session, with a fake backend. Run: bun test
import { expect, test } from 'bun:test'
import { DEFAULT_CONFIGS } from '../src/constants'
import type { DatabaseApi } from '../src/services/DatabaseApi'
import { Session } from '../src/services/Session'
import { FakePrompt } from './FakePrompt'
import type { DbUser, GrantableAccess, NewUser, TableRef, UserList } from '../src/types'

const ROOT: DbUser = { name: 'root', host: '%', access: 'admin', current: true }
const BOB: DbUser = { name: 'bob', host: 'localhost', access: 'read', current: false }
const ORDERS: TableRef = { schema: null, name: 'orders', kind: 'table' }

class FakeApi {
  canManage = true
  /** When true, `listUsers` fails. */
  broken = false
  lists = 0
  calls: unknown[][] = []

  async connect(): Promise<void> {}

  async disconnect(): Promise<void> {}

  async useDatabase(): Promise<void> {}

  async listDatabases(): Promise<string[]> {
    return ['shop']
  }

  async listTables(): Promise<TableRef[]> {
    return [ORDERS]
  }

  async browse(): Promise<{ columns: never[]; rows: never[]; total: number }> {
    return { columns: [], rows: [], total: 0 }
  }

  async listUsers(): Promise<UserList> {
    this.lists += 1
    if (this.broken) {
      throw 'access denied'
    }
    return { canManage: this.canManage, users: [ROOT, BOB] }
  }

  async createUser(_id: string, user: NewUser): Promise<void> {
    this.calls.push(['create', user])
  }

  async dropUser(_id: string, user: DbUser): Promise<void> {
    this.calls.push(['drop', user.name, user.host])
  }

  async setUserAccess(_id: string, user: DbUser, access: GrantableAccess): Promise<void> {
    if (user.current) {
      throw 'You cannot change your own access'
    }
    this.calls.push(['access', user.name, user.host, access])
  }

  async setUserPassword(_id: string, user: DbUser, password: string): Promise<void> {
    if (password === '') {
      throw 'The password cannot be empty'
    }
    this.calls.push(['password', user.name, user.host, password])
  }
}

async function connected(api: FakeApi): Promise<Session> {
  const session = new Session(api as unknown as DatabaseApi, new FakePrompt())
  await session.connect(DEFAULT_CONFIGS.mysql, 'test')
  return session
}

test('connect loads the users and sets canManageUsers', async function () {
  const session = await connected(new FakeApi())
  expect(session.canManageUsers).toBe(true)
  expect(session.users).toEqual([ROOT, BOB])
})

test('a failing user list does not break connect', async function () {
  const api = new FakeApi()
  api.broken = true
  const session = new Session(api as unknown as DatabaseApi, new FakePrompt())
  expect(await session.run(session.connect(DEFAULT_CONFIGS.mysql, 'test'))).toBe(true)
  expect(session.error).toBe('')
  expect(session.config).not.toBeNull()
  expect(session.tables).toEqual([ORDERS])
  expect(session.canManageUsers).toBe(false)
  expect(session.users).toEqual([])
})

test('create, drop and set access call the api and load the list again', async function () {
  const api = new FakeApi()
  const session = await connected(api)
  const ann: NewUser = { name: 'ann', host: '%', password: 'secret', access: 'write' }

  await session.createUser(ann)
  expect(session.status).toBe('User "ann" added')
  expect(api.lists).toBe(2)

  await session.setUserAccess(BOB, 'none')
  expect(session.status).toBe('Access of "bob" set to None')
  expect(api.lists).toBe(3)

  await session.dropUser(BOB)
  expect(session.status).toBe('User "bob" removed')
  expect(api.lists).toBe(4)

  expect(api.calls).toEqual([
    ['create', ann],
    ['access', 'bob', 'localhost', 'none'],
    ['drop', 'bob', 'localhost'],
  ])
})

test('a password change calls the api and keeps the password out of the status', async function () {
  const api = new FakeApi()
  const session = await connected(api)

  await session.setUserPassword(BOB, 'new-secret')
  expect(api.calls).toEqual([['password', 'bob', 'localhost', 'new-secret']])
  expect(session.status).toBe('Password of "bob" changed')
  expect(session.status).not.toContain('new-secret')
  // The list is not loaded again.
  expect(api.lists).toBe(1)

  // The own password can be changed too.
  await session.setUserPassword(ROOT, 'other-secret')
  expect(api.calls[1]).toEqual(['password', 'root', '%', 'other-secret'])

  expect(await session.run(session.setUserPassword(BOB, ''))).toBe(false)
  expect(session.error).toBe('The password cannot be empty')
})

test('an error of the backend is shown', async function () {
  const session = await connected(new FakeApi())
  expect(await session.run(session.setUserAccess(ROOT, 'read'))).toBe(false)
  expect(session.error).toBe('You cannot change your own access')
})

test('the Users view falls back to Content when users become unavailable', async function () {
  const api = new FakeApi()
  const session = await connected(api)
  session.view = 'users'

  // The same account may manage users on the next database too.
  await session.useDatabase('shop')
  expect(session.view).toBe('users')

  api.canManage = false
  await session.useDatabase('other')
  expect(session.canManageUsers).toBe(false)
  expect(session.view).toBe('content')
})

test('the Users view is left on a table click and on disconnect', async function () {
  const session = await connected(new FakeApi())
  session.view = 'users'
  await session.selectTable(ORDERS)
  expect(session.view).toBe('content')

  session.view = 'users'
  await session.disconnect()
  expect(session.view).toBe('content')
  expect(session.users).toEqual([])
  expect(session.canManageUsers).toBe(false)
})
