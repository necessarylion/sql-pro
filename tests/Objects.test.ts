// Checks views, triggers and routines in the Session, with a fake backend. Run: bun test
import { expect, test } from 'bun:test'
import type { DatabaseApi } from '../src/services/DatabaseApi'
import { Session } from '../src/services/Session'
import { FakePrompt } from './FakePrompt'
import type { DbObject, TablePage, TableRef } from '../src/types'

const USERS: TableRef = { schema: null, name: 'users', kind: 'table' }
const ACTIVE: TableRef = { schema: null, name: 'active_users', kind: 'view' }
const TRIGGER: DbObject = {
  kind: 'trigger',
  schema: null,
  name: 'audit',
  table: 'users',
  args: null,
}
const FUNCTION: DbObject = {
  kind: 'function',
  schema: 'public',
  name: 'add_one',
  table: null,
  args: 'a integer',
}

class FakeApi {
  objects = [TRIGGER, FUNCTION]
  failObjects = false
  dropped: DbObject[] = []

  async listTables(): Promise<TableRef[]> {
    return [USERS, ACTIVE]
  }

  async listObjects(): Promise<DbObject[]> {
    if (this.failObjects) {
      throw 'access denied'
    }
    return [...this.objects]
  }

  async objectDefinition(_id: string, object: DbObject): Promise<string> {
    return `CREATE ${object.kind.toUpperCase()} ${object.name}`
  }

  async dropObject(_id: string, object: DbObject): Promise<void> {
    this.dropped.push(object)
    this.objects = this.objects.filter(function (known) {
      return known.name !== object.name
    })
  }

  async dropTable(): Promise<void> {}

  async browse(): Promise<TablePage> {
    return { columns: [], rows: [], total: 0 }
  }
}

async function connected(): Promise<{ api: FakeApi; session: Session; prompt: FakePrompt }> {
  const api = new FakeApi()
  const prompt = new FakePrompt()
  const session = new Session(api as unknown as DatabaseApi, prompt)
  await session.loadTables()
  return { api, session, prompt }
}

test('the table list also loads the triggers and routines', async function () {
  const { session } = await connected()
  expect(session.objects).toEqual([TRIGGER, FUNCTION])
})

test('a failed object list keeps the tables', async function () {
  const api = new FakeApi()
  api.failObjects = true
  const session = new Session(api as unknown as DatabaseApi, new FakePrompt())
  expect(await session.run(session.loadTables())).toBe(true)
  expect(session.tables.length).toBe(2)
  expect(session.objects).toEqual([])
})

test('a definition replaces the open table and leaves the Query view', async function () {
  const { session } = await connected()
  await session.selectTable(USERS)
  session.view = 'query'
  await session.showDefinition(TRIGGER)
  expect(session.table).toBeNull()
  expect(session.view).toBe('content')
  expect(session.definition?.sql).toBe('CREATE TRIGGER audit')
  expect(session.shows(TRIGGER)).toBe(true)
  expect(session.shows(FUNCTION)).toBe(false)

  // Opening a table hides the definition again.
  await session.selectTable(USERS)
  expect(session.definition).toBeNull()
})

test('a definition asks before unsaved changes are lost', async function () {
  const { session, prompt } = await connected()
  await session.selectTable(USERS)
  session.addRow()
  prompt.answer = false
  await session.showDefinition(TRIGGER)
  expect(prompt.questions.length).toBe(1)
  expect(session.definition).toBeNull()
  expect(session.table).toEqual(USERS)
})

test('the definition can be edited in the Query view', async function () {
  const { session } = await connected()
  await session.showDefinition(FUNCTION)
  session.editDefinition()
  expect(session.view).toBe('query')
  expect(session.sql).toBe('CREATE FUNCTION add_one')
})

test('dropping the shown object hides it and loads the list again', async function () {
  const { api, session } = await connected()
  await session.showDefinition(FUNCTION)
  await session.dropObject(FUNCTION)
  expect(api.dropped).toEqual([FUNCTION])
  expect(session.definition).toBeNull()
  expect(session.objects).toEqual([TRIGGER])
  expect(session.status).toBe('Function "add_one" dropped')
})

test('dropping a view hides its definition', async function () {
  const { session } = await connected()
  await session.showDefinition(Session.viewObject(ACTIVE))
  expect(session.definition?.sql).toBe('CREATE VIEW active_users')
  await session.dropTables([ACTIVE])
  expect(session.definition).toBeNull()
})
