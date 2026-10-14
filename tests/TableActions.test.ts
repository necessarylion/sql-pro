// Checks create, rename, drop and truncate of the Session, with a fake backend. Run: bun test
import { expect, test } from 'bun:test'
import type { DatabaseApi } from '../src/services/DatabaseApi'
import { Session } from '../src/services/Session'
import { TableDraft } from '../src/services/TableDraft'
import { FakePrompt } from './FakePrompt'
import type { BrowseRequest, NewColumn, TablePage, TableRef } from '../src/types'

const USERS: TableRef = { schema: null, name: 'users', kind: 'table' }
const ORDERS: TableRef = { schema: null, name: 'orders', kind: 'table' }

class FakeApi {
  tables = [USERS, ORDERS]
  truncated: string[] = []
  created: { name: string; columns: NewColumn[] }[] = []
  requests: BrowseRequest[] = []

  async listTables(): Promise<TableRef[]> {
    return [...this.tables]
  }

  async browse(_id: string, request: BrowseRequest): Promise<TablePage> {
    this.requests.push(request)
    return { columns: [], rows: [], total: 0 }
  }

  async createTable(_id: string, name: string, columns: NewColumn[]): Promise<void> {
    this.created.push({ name, columns })
    this.tables.push({ schema: null, name, kind: 'table' })
  }

  async renameTable(_id: string, table: TableRef, name: string): Promise<void> {
    const index = this.tables.findIndex(function (known) {
      return known.name === table.name
    })
    if (index < 0) {
      throw 'Unknown table'
    }
    this.tables[index] = { ...table, name }
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

async function openUsers(): Promise<{ api: FakeApi; session: Session; prompt: FakePrompt }> {
  const api = new FakeApi()
  const prompt = new FakePrompt()
  const session = new Session(api as unknown as DatabaseApi, prompt)
  await session.loadTables()
  await session.selectTable(USERS)
  return { api, session, prompt }
}

/** Gives the open table one unsaved change. */
function addUnsavedRow(session: Session): void {
  session.page = { columns: [], rows: [], total: 0 }
  session.addRow()
}

test('creating a table reloads the list and opens it on the Structure view', async function () {
  const { api, session } = await openUsers()
  const label = { ...TableDraft.emptyColumn(), name: ' label ', dataType: 'TEXT' }
  expect(await session.run(session.createTable(' logs ', [TableDraft.idColumn(), label]))).toBe(
    true,
  )
  expect(api.created.length).toBe(1)
  expect(api.created[0].name).toBe('logs')
  expect(api.created[0].columns[1]).toEqual({
    name: 'label',
    dataType: 'TEXT',
    nullable: true,
    default: null,
    primaryKey: false,
    autoIncrement: false,
  })
  expect(session.tables.length).toBe(3)
  expect(session.table?.name).toBe('logs')
  expect(session.view).toBe('structure')
  expect(session.status).toBe('Table "logs" created')
})

test('a bad new table is refused and nothing is sent', async function () {
  const { api, session } = await openUsers()
  const twice = [
    TableDraft.idColumn(),
    { ...TableDraft.emptyColumn(), name: 'id', dataType: 'TEXT' },
  ]
  expect(await session.run(session.createTable('logs', twice))).toBe(false)
  expect(session.error).toBe('Two columns are named "id"')
  expect(await session.run(session.createTable('logs', []))).toBe(false)
  expect(await session.run(session.createTable(' ', [TableDraft.idColumn()]))).toBe(false)
  expect(api.created).toEqual([])
  expect(session.table).toBe(USERS)
})

test('a new table is made but not opened when unsaved changes are kept', async function () {
  const { session, prompt } = await openUsers()
  addUnsavedRow(session)
  prompt.answer = false
  await session.createTable('logs', [TableDraft.idColumn()])
  expect(session.tables.length).toBe(3)
  expect(session.table).toBe(USERS)
  expect(session.view).toBe('content')
  expect(session.pendingCount).toBe(1)
})

test('renaming the open table keeps it open under the new name', async function () {
  const { api, session } = await openUsers()
  session.orderBy = 'name'
  // A copy, like the one a refreshed list gives. It is still the open table.
  await session.renameTable({ ...USERS }, ' people ')
  expect(session.table).toEqual({ schema: null, name: 'people', kind: 'table' })
  expect(session.isOpen(session.tables[0])).toBe(true)
  expect(session.page).not.toBeNull()
  expect(session.orderBy).toBe('name')
  expect(session.status).toBe('Table "users" renamed to "people"')
  // The rows are the same, so they are not loaded again.
  expect(api.requests.length).toBe(1)
})

test('renaming another table keeps the open one, a bad name is refused', async function () {
  const { session } = await openUsers()
  await session.renameTable(ORDERS, 'sales')
  expect(session.table).toBe(USERS)
  expect(session.tables[1].name).toBe('sales')

  expect(await session.run(session.renameTable(USERS, '  '))).toBe(false)
  expect(session.error).toBe('The table needs a name')
  // The old name again: nothing to do.
  session.status = ''
  await session.renameTable(USERS, 'users')
  expect(session.status).toBe('')
  expect(await session.run(session.renameTable(ORDERS, 'gone'))).toBe(false)
  expect(session.error).toBe('Unknown table')
})

test('renaming the open table asks before unsaved changes are lost', async function () {
  const { session, prompt } = await openUsers()
  addUnsavedRow(session)
  prompt.answer = false
  await session.renameTable(USERS, 'people')
  expect(prompt.questions.length).toBe(1)
  expect(session.tables[0].name).toBe('users')
  expect(session.pendingCount).toBe(1)

  prompt.answer = true
  await session.renameTable(USERS, 'people')
  expect(session.table?.name).toBe('people')
  expect(session.pendingCount).toBe(0)
})

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

test('dropping many tables asks once, reloads the list once and says how many', async function () {
  const { api, session, prompt } = await openUsers()
  addUnsavedRow(session)
  prompt.answer = true
  await session.dropTables([ORDERS, { ...USERS }])
  expect(prompt.questions.length).toBe(1)
  expect(api.tables).toEqual([])
  expect(session.tables).toEqual([])
  expect(session.table).toBeNull()
  expect(session.status).toBe('2 tables dropped')
})

test('dropping many tables stops at the first error and still reloads the list', async function () {
  const { api, session } = await openUsers()
  const missing: TableRef = { schema: null, name: 'missing', kind: 'table' }
  const view: TableRef = { schema: null, name: 'recent', kind: 'view' }
  expect(await session.run(session.dropTables([ORDERS, missing, view]))).toBe(false)
  expect(session.error).toBe('Could not drop "missing" (1 of 3 done): Unknown table')
  expect(api.tables).toEqual([USERS])
  expect(session.tables).toEqual([USERS])
})

test('truncating many tables reloads the open table once', async function () {
  const { api, session } = await openUsers()
  await session.truncateTables([ORDERS, { ...USERS }])
  expect(api.truncated).toEqual(['orders', 'users'])
  expect(api.requests.length).toBe(2)
  expect(session.status).toBe('2 tables truncated')
})
