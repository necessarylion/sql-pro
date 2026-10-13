// Checks drop and truncate of the Session, with a fake backend. Run: bun test
import { expect, test } from 'bun:test'
import type { DatabaseApi } from '../src/services/DatabaseApi'
import { Session } from '../src/services/Session'
import type { BrowseRequest, TablePage, TableRef } from '../src/types'

const USERS: TableRef = { schema: null, name: 'users', kind: 'table' }
const ORDERS: TableRef = { schema: null, name: 'orders', kind: 'table' }

class FakeApi {
  tables = [USERS, ORDERS]
  truncated: string[] = []
  requests: BrowseRequest[] = []

  async listTables(): Promise<TableRef[]> {
    return [...this.tables]
  }

  async browse(_id: string, request: BrowseRequest): Promise<TablePage> {
    this.requests.push(request)
    return { columns: [], rows: [], total: 0 }
  }

  async dropTable(_id: string, table: TableRef): Promise<void> {
    const index = this.tables.findIndex(function (known) {
      return known.name === table.name
    })
    if (index < 0) {
      throw 'Unknown table'
    }
    this.tables.splice(index, 1)
  }

  async truncateTable(_id: string, table: TableRef): Promise<void> {
    this.truncated.push(table.name)
  }
}

async function openUsers(): Promise<{ api: FakeApi; session: Session }> {
  const api = new FakeApi()
  const session = new Session(api as unknown as DatabaseApi)
  await session.loadTables()
  await session.selectTable(USERS)
  return { api, session }
}

test('dropping the open table clears it and reloads the list', async function () {
  const { session } = await openUsers()
  // A copy, like the one a refreshed list gives. It is still the open table.
  expect(await session.run(session.dropTable({ ...USERS }))).toBe(true)
  expect(session.table).toBeNull()
  expect(session.page).toBeNull()
  expect(session.tables).toEqual([ORDERS])
  expect(session.status).toBe('Table "users" dropped')
})

test('dropping another table keeps the open one, a failed drop shows the error', async function () {
  const { session } = await openUsers()
  await session.dropTable(ORDERS)
  expect(session.table).toBe(USERS)
  expect(session.page).not.toBeNull()
  expect(session.tables).toEqual([USERS])

  expect(await session.run(session.dropTable(ORDERS))).toBe(false)
  expect(session.error).toBe('Unknown table')
})

test('truncating the open table reloads its first page', async function () {
  const { api, session } = await openUsers()
  session.offset = 500
  await session.truncateTable({ ...USERS })
  expect(api.truncated).toEqual(['users'])
  expect(api.requests.length).toBe(2)
  expect(api.requests[1].offset).toBe(0)
  expect(session.status).toBe('Table "users" truncated')

  // Another table: nothing to reload.
  await session.truncateTable(ORDERS)
  expect(api.requests.length).toBe(2)
})
