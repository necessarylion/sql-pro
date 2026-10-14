// One small check of the Session logic, with a fake backend. Run: bun test
import { expect, test } from 'bun:test'
import { isReactive, markRaw, reactive } from 'vue'
import type { DatabaseApi } from '../src/services/DatabaseApi'
import { Session } from '../src/services/Session'
import { RAW_FILTER_COLUMN } from '../src/constants/filters'
import type {
  BrowseRequest,
  ConnectionConfig,
  ResultSet,
  Row,
  RowUpdate,
  SaveResult,
  TablePage,
  TableRef,
  UserList,
  Values,
} from '../src/types'
import { FakePrompt } from './FakePrompt'

const USERS: TableRef = { schema: null, name: 'users', kind: 'table' }

class FakeApi {
  saves: { updates: RowUpdate[]; inserts: Values[]; deletes: Values[] }[] = []
  failSave = false
  rows: Row[] = [
    ['1', 'Ann'],
    ['2', null],
  ]
  requests: BrowseRequest[] = []

  async browse(_id: string, request: BrowseRequest): Promise<TablePage> {
    this.requests.push(request)
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
        {
          name: 'name',
          dataType: 'text',
          nullable: true,
          default: null,
          primaryKey: false,
          extra: '',
        },
      ],
      rows: structuredClone(this.rows),
      total: this.rows.length,
    }
  }

  async saveChanges(
    _id: string,
    _table: TableRef,
    updates: RowUpdate[],
    inserts: Values[],
    deletes: Values[],
  ): Promise<SaveResult> {
    if (this.failSave) {
      throw 'Row 1 could not be saved: bad value'
    }
    // A copy, as the session clears its own lists after the call.
    this.saves.push(structuredClone({ updates, inserts, deletes }))
    return { updated: updates.length, inserted: inserts.length, deleted: deletes.length }
  }

  cancels = 0
  /** A cancelled query ends with rows, not an error. MySQL does this for SLEEP. */
  cancelEndsOk = false
  /** The query named 'slow'. It runs until it is cancelled. */
  private slow: PromiseWithResolvers<ResultSet[]> | null = null

  async cancelQuery(): Promise<void> {
    this.cancels += 1
    if (this.cancelEndsOk) {
      this.slow?.resolve([{ columns: ['x'], rows: [['1']], affected: 0, truncated: false }])
    } else {
      this.slow?.reject('Query cancelled')
    }
  }

  async runQuery(_id: string, sql: string): Promise<ResultSet[]> {
    if (sql === 'slow') {
      this.slow = Promise.withResolvers<ResultSet[]>()
      return this.slow.promise
    }
    if (sql === 'bad') {
      throw 'syntax error'
    }
    return [
      { columns: [], rows: [], affected: 3, truncated: false },
      { columns: ['id'], rows: [['1'], ['2']], affected: 0, truncated: true },
    ]
  }

  async listTables(): Promise<TableRef[]> {
    return [USERS]
  }
}

async function openUsers(): Promise<{ api: FakeApi; session: Session; prompt: FakePrompt }> {
  const api = new FakeApi()
  const prompt = new FakePrompt()
  const session = new Session(api as unknown as DatabaseApi, prompt)
  session.view = 'query'
  await session.selectTable(USERS)
  return { api, session, prompt }
}

test('Vue does not wrap the loaded rows, but new rows stay reactive', async function () {
  // markRaw: the fake keeps its rows in a field, and a Proxy cannot be cloned.
  const api = markRaw(new FakeApi())
  const session = reactive(new Session(api as unknown as DatabaseApi, new FakePrompt())) as Session
  await session.selectTable(USERS)
  await session.runQuery('select 1')
  // Big row data stays plain. Reading it in the grid is then fast.
  expect(isReactive(session.page)).toBe(false)
  expect(isReactive(session.page?.rows)).toBe(false)
  expect(isReactive(session.result)).toBe(false)
  // A new row is changed in place, so Vue must still see it.
  session.addRow()
  session.updateCell(session.page?.rows.length ?? 0, 1, 'new')
  expect(isReactive(session.newRows[0])).toBe(true)
  expect(session.shownRows.at(-1)).toEqual([null, 'new'])
})

test('a cell edit is pending and is not sent', async function () {
  const { api, session } = await openUsers()
  expect(session.view).toBe('content')
  expect(session.editable).toBe(true)

  session.updateCell(1, 1, 'Bob')
  expect(api.saves).toEqual([])
  expect(session.pendingCount).toBe(1)
  expect(session.unsavedText).toBe('1 unsaved change')
  // The grid shows the new value. The loaded page is not changed.
  expect(session.shownRows[1]).toEqual(['2', 'Bob'])
  expect(session.page?.rows[1]).toEqual(['2', null])
  expect(session.changedCells).toEqual({ 1: [1] })

  // Back to the loaded value: nothing is pending any more.
  session.updateCell(1, 1, null)
  expect(session.pendingCount).toBe(0)
  expect(session.edits).toEqual({})
  expect(session.changedCells).toEqual({})
})

test('saveChanges sends one update per row with the original key, and the new rows', async function () {
  const { api, session } = await openUsers()
  // Two cells of one row. One of them is the primary key.
  session.updateCell(0, 0, '9')
  session.updateCell(0, 1, 'Anna')
  session.updateCell(1, 1, 'Bob')
  session.addRow()
  session.addRow()
  session.updateCell(2, 1, 'Cy')
  expect(session.pendingCount).toBe(5)
  expect(session.shownRows.length).toBe(4)

  expect(await session.run(session.saveChanges())).toBe(true)
  expect(api.saves).toEqual([
    {
      updates: [
        { key: { id: '1' }, changes: { id: '9', name: 'Anna' } },
        { key: { id: '2' }, changes: { name: 'Bob' } },
      ],
      // Only the columns that were set. The second new row is empty.
      inserts: [{ name: 'Cy' }, {}],
      deletes: [],
    },
  ])
  expect(session.pendingCount).toBe(0)
  expect(session.newRows).toEqual([])
  expect(session.status).toBe('2 rows updated, 2 rows added')
  // The first load and the reload after the save.
  expect(api.requests.length).toBe(2)

  // Nothing is pending now, so nothing is sent.
  await session.saveChanges()
  expect(api.saves.length).toBe(1)
})

test('a failed save keeps the pending changes, discard drops them', async function () {
  const { api, session } = await openUsers()
  session.updateCell(0, 1, 'Anna')
  session.addRow()
  api.failSave = true

  expect(await session.run(session.saveChanges())).toBe(false)
  expect(session.error).toBe('Row 1 could not be saved: bad value')
  expect(session.pendingCount).toBe(2)
  expect(session.shownRows[0]).toEqual(['1', 'Anna'])
  expect(api.requests.length).toBe(1)

  session.discardChanges()
  expect(session.pendingCount).toBe(0)
  expect(session.shownRows).toEqual(session.page?.rows ?? [])
})

test('the guard asks before the page changes and keeps the changes on "no"', async function () {
  const { api, session, prompt } = await openUsers()
  // Nothing is pending: no question.
  await session.sort('name')
  expect(prompt.questions).toEqual([])
  expect(api.requests.length).toBe(2)

  session.updateCell(0, 1, 'Anna')
  session.addRow()
  prompt.answer = false
  await session.sort('id')
  await session.reload()
  await session.applyFilters()
  await session.selectTable({ ...USERS })
  await session.disconnect()
  expect(prompt.questions.length).toBe(5)
  expect(prompt.questions[0]).toBe('You have 2 unsaved changes in "users". Discard them?')
  expect(api.requests.length).toBe(2)
  expect(session.orderBy).toBe('name')
  expect(session.pendingCount).toBe(2)

  prompt.answer = true
  await session.sort('id')
  expect(api.requests.length).toBe(3)
  expect(session.orderBy).toBe('id')
  expect(session.pendingCount).toBe(0)
})

test('deleting a row only marks it, and deleting it again restores it', async function () {
  const { api, session, prompt } = await openUsers()
  session.updateCell(0, 1, 'Anna')
  session.updateCell(1, 1, 'Bob')
  session.addRow()
  session.addRow()
  session.updateCell(3, 1, 'kept')

  // Row 1, which has an edit, and the first new row.
  expect(session.restores([1, 2])).toBe(false)
  session.deleteRows([1, 2])
  // Nothing is asked and nothing is sent.
  expect(prompt.questions).toEqual([])
  expect(api.saves).toEqual([])
  // The new row is gone. The other new row stays.
  expect(session.newRows).toEqual([[null, 'kept']])
  // The marked row stays in the grid, with its loaded values. Its edit is dropped.
  expect(session.deletedRows).toEqual([1])
  expect(session.shownRows[1]).toEqual(['2', null])
  expect(Object.values(session.edits)).toEqual([{ key: { id: '1' }, changes: { name: 'Anna' } }])
  // One edit, one new row, one delete.
  expect(session.pendingCount).toBe(3)

  // A marked row cannot be edited.
  session.updateCell(1, 1, 'Bob')
  expect(session.shownRows[1]).toEqual(['2', null])
  expect(session.pendingCount).toBe(3)

  // A marked row and a row without a mark: the row without a mark gets one too.
  expect(session.restores([0, 1])).toBe(false)
  expect(session.restores([])).toBe(false)
  // Only marked rows: the delete action restores them.
  expect(session.restores([1])).toBe(true)
  session.deleteRows([1])
  expect(session.deletedRows).toEqual([])
  expect(session.deletes).toEqual({})
  expect(session.pendingCount).toBe(2)

  // Discard clears the marks too.
  session.deleteRows([0, 1])
  expect(session.deletedRows).toEqual([0, 1])
  expect(session.edits).toEqual({})
  session.discardChanges()
  expect(session.deletedRows).toEqual([])
  expect(session.pendingCount).toBe(0)
})

test('saveChanges asks once, then sends the deletes with the edits and the new rows', async function () {
  const { api, session, prompt } = await openUsers()
  session.updateCell(0, 1, 'Anna')
  session.addRow()
  session.deleteRows([1])
  expect(session.unsavedText).toBe('3 unsaved changes')

  // "No" sends nothing and keeps everything.
  prompt.answer = false
  expect(await session.run(session.saveChanges())).toBe(true)
  expect(prompt.questions).toEqual([
    'Saving will delete 1 row from "users". This cannot be undone. Continue?',
  ])
  expect(api.saves).toEqual([])
  expect(session.pendingCount).toBe(3)

  prompt.answer = true
  expect(await session.run(session.saveChanges())).toBe(true)
  expect(prompt.questions.length).toBe(2)
  expect(api.saves).toEqual([
    {
      updates: [{ key: { id: '1' }, changes: { name: 'Anna' } }],
      inserts: [{}],
      deletes: [{ id: '2' }],
    },
  ])
  expect(session.status).toBe('1 row updated, 1 row added, 1 row deleted')
  expect(session.pendingCount).toBe(0)
  expect(session.deletedRows).toEqual([])

  // A save without deletes does not ask.
  session.updateCell(0, 1, 'Al')
  await session.saveChanges()
  expect(prompt.questions.length).toBe(2)
  expect(api.saves.length).toBe(2)
})

test('a failed save keeps the delete marks, and the guard counts them', async function () {
  const { api, session, prompt } = await openUsers()
  session.updateCell(0, 1, 'Anna')
  session.deleteRows([1])
  api.failSave = true

  expect(await session.run(session.saveChanges())).toBe(false)
  expect(session.error).toBe('Row 1 could not be saved: bad value')
  expect(session.deletedRows).toEqual([1])
  expect(session.pendingCount).toBe(2)
  // No reload after the failed save.
  expect(api.requests.length).toBe(1)

  // A delete mark alone is an unsaved change: the page may not go without a question.
  session.updateCell(0, 1, 'Ann')
  expect(session.pendingCount).toBe(1)
  prompt.answer = false
  await session.reload()
  expect(prompt.questions[prompt.questions.length - 1]).toBe(
    'You have 1 unsaved change in "users". Discard it?',
  )
  expect(session.deletedRows).toEqual([1])

  // "Yes" on the reload clears the mark.
  prompt.answer = true
  await session.reload()
  expect(session.deletedRows).toEqual([])
  expect(api.requests.length).toBe(2)
})

test('runQuery keeps the last set with columns and reports the rest', async function () {
  const { session } = await openUsers()
  expect(await session.run(session.runQuery('select 1'))).toBe(true)
  expect(session.result?.rows.length).toBe(2)
  expect(session.status).toMatch(/^2 rows \(truncated\), 3 rows affected in \d+\.\d\ds$/)

  expect(await session.run(session.runQuery('bad'))).toBe(false)
  expect(session.error).toBe('syntax error')
  expect(session.busy).toBe(false)
})

test('cancel stops a running query and shows a short status, not an error', async function () {
  const { api, session } = await openUsers()
  const run = session.run(session.runQuery('slow'))
  expect(session.busy).toBe(true)
  expect(session.cancellable).toBe(true)
  const cancel = session.cancel()
  expect(session.cancelling).toBe(true)
  // A second press while the first cancel is on its way sends nothing more.
  await session.cancel()
  await cancel
  expect(await run).toBe(false)
  expect(api.cancels).toBe(1)
  expect(session.error).toBe('')
  expect(session.status).toBe('Query cancelled')
  expect(session.busy).toBe(false)
  expect(session.cancellable).toBe(false)
  expect(session.cancelling).toBe(false)
})

test('a cancelled query that still ends with rows also shows the short status', async function () {
  const { api, session } = await openUsers()
  api.cancelEndsOk = true
  const run = session.run(session.runQuery('slow'))
  await session.cancel()
  expect(await run).toBe(false)
  expect(session.status).toBe('Query cancelled')
  expect(session.error).toBe('')
})

test('cancel does nothing when no query runs', async function () {
  const { api, session } = await openUsers()
  await session.cancel()
  expect(api.cancels).toBe(0)
  // A finished query is no longer cancellable, and the next run shows its normal status.
  expect(await session.run(session.runQuery('select 1'))).toBe(true)
  expect(session.cancellable).toBe(false)
  await session.cancel()
  expect(api.cancels).toBe(0)
  expect(session.status).toMatch(/^2 rows/)
})

test('only ticked and complete filter rows are sent, raw rows go to filter', async function () {
  const { api, session } = await openUsers()
  // The first row starts on the first column of the table.
  expect(session.filterRows).toEqual([{ enabled: true, column: 'id', operator: 'eq', value: '' }])

  session.filterRows = [
    { enabled: true, column: 'id', operator: 'gt', value: '1' },
    { enabled: false, column: 'id', operator: 'eq', value: '9' },
    { enabled: true, column: 'name', operator: 'contains', value: '' },
    { enabled: true, column: 'name', operator: 'isNull', value: 'not used' },
    { enabled: true, column: RAW_FILTER_COLUMN, operator: 'eq', value: ' id < 5 ' },
    { enabled: true, column: RAW_FILTER_COLUMN, operator: 'eq', value: "name = 'a' OR id = 2" },
    { enabled: false, column: RAW_FILTER_COLUMN, operator: 'eq', value: 'id = 7' },
  ]
  session.offset = 500
  await session.applyFilters()

  const request = api.requests[api.requests.length - 1]
  expect(request.offset).toBe(0)
  expect(request.filters).toEqual([
    { column: 'id', operator: 'gt', value: '1' },
    { column: 'name', operator: 'isNull', value: '' },
  ])
  expect(request.filter).toBe("(id < 5) AND (name = 'a' OR id = 2)")
  expect(session.filtered).toBe(true)

  // Removing every row leaves one empty row and no filter.
  while (session.filtered) {
    await session.removeFilterRow(0)
  }
  expect(session.filterRows.length).toBeGreaterThan(0)
  expect(api.requests[api.requests.length - 1].filters).toEqual([])
  expect(api.requests[api.requests.length - 1].filter).toBe('')
})

test('a page turn sends the known total, other loads do not', async function () {
  const { api, session } = await openUsers()
  expect(api.requests[0].total).toBeUndefined()
  // Pretend the table is big, so there is a next page.
  session.page!.total = 1000

  await session.turnPage(1)
  expect(api.requests[api.requests.length - 1].total).toBe(1000)
  await session.turnPage(-1)
  expect(api.requests[api.requests.length - 1].total).toBe(2)

  await session.sort('name')
  expect(api.requests[api.requests.length - 1].total).toBeUndefined()
  await session.applyFilters()
  expect(api.requests[api.requests.length - 1].total).toBeUndefined()
  await session.reload()
  expect(api.requests[api.requests.length - 1].total).toBeUndefined()
  expect(api.requests.length).toBe(6)
})

const CONFIG: ConnectionConfig = {
  driver: 'mysql',
  host: 'db',
  port: 3306,
  user: 'root',
  password: 'secret',
  database: 'shop',
  ssh: null,
}

/** A backend for `connect`. It notes if the config was set while it loaded. */
class ConnectApi {
  session: Session | null = null
  configSeen: (ConnectionConfig | null)[] = []
  disconnects = 0
  failTables = false

  async connect(): Promise<void> {}

  async disconnect(): Promise<void> {
    this.disconnects += 1
  }

  async listDatabases(): Promise<string[]> {
    this.configSeen.push(this.session?.config ?? null)
    return ['shop']
  }

  async listTables(): Promise<TableRef[]> {
    this.configSeen.push(this.session?.config ?? null)
    if (this.failTables) {
      throw 'timeout'
    }
    return [USERS]
  }

  async listObjects(): Promise<[]> {
    return []
  }

  async listUsers(): Promise<UserList> {
    this.configSeen.push(this.session?.config ?? null)
    return { canManage: false, users: [] }
  }
}

function connectSession(api: ConnectApi): Session {
  const session = new Session(api as unknown as DatabaseApi, new FakePrompt())
  api.session = session
  return session
}

test('connect sets the config only after everything is loaded', async function () {
  const api = new ConnectApi()
  const session = connectSession(api)
  await session.connect(CONFIG, 'Shop')
  // The form with its spinner stays while the lists load.
  expect(api.configSeen).toEqual([null, null, null])
  expect(session.config?.database).toBe('shop')
  expect(session.config?.password).toBe('')
  expect(session.title).toBe('Shop')
  expect(session.tables).toEqual([USERS])
  expect(api.disconnects).toBe(0)
})

test('connect closes the connection when loading fails', async function () {
  const api = new ConnectApi()
  api.failTables = true
  const session = connectSession(api)
  await expect(session.connect(CONFIG, 'Shop')).rejects.toBe('timeout')
  expect(session.config).toBeNull()
  expect(api.disconnects).toBe(1)
})

test('the tab stays busy until the last of overlapping runs ends', async function () {
  const session = connectSession(new ConnectApi())
  const slow = Promise.withResolvers<void>()
  // A double click: a short load ends while the connect still runs.
  const connect = session.run(slow.promise)
  await session.run(Promise.resolve())
  expect(session.busy).toBe(true)
  slow.resolve()
  await connect
  expect(session.busy).toBe(false)
})
