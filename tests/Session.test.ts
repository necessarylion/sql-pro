// One small check of the Session logic, with a fake backend. Run: bun test
import { expect, test } from 'bun:test'
import type { DatabaseApi } from '../src/services/DatabaseApi'
import { Session } from '../src/services/Session'
import { RAW_FILTER_COLUMN } from '../src/constants/filters'
import type { BrowseRequest, ResultSet, TablePage, TableRef, Values } from '../src/types'

const USERS: TableRef = { schema: null, name: 'users', kind: 'table' }

class FakeApi {
  updates: { key: Values; changes: Values }[] = []
  inserts: Values[] = []
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
      rows: [
        ['1', 'Ann'],
        ['2', null],
      ],
      total: 2,
    }
  }

  async updateRow(_id: string, _table: TableRef, key: Values, changes: Values): Promise<number> {
    this.updates.push({ key, changes })
    return 1
  }

  async insertRow(_id: string, _table: TableRef, values: Values): Promise<number> {
    this.inserts.push(values)
    return 1
  }

  async runQuery(_id: string, sql: string): Promise<ResultSet[]> {
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

async function openUsers(): Promise<{ api: FakeApi; session: Session }> {
  const api = new FakeApi()
  const session = new Session(api as unknown as DatabaseApi)
  session.view = 'query'
  await session.selectTable(USERS)
  return { api, session }
}

test('a cell edit sends the primary key and only the changed column', async function () {
  const { api, session } = await openUsers()
  expect(session.view).toBe('content')
  expect(session.editable).toBe(true)

  await session.updateCell(1, 1, 'Bob')
  expect(api.updates).toEqual([{ key: { id: '2' }, changes: { name: 'Bob' } }])
})

test('a draft row is changed locally and saved without its NULL columns', async function () {
  const { api, session } = await openUsers()
  session.addDraft()
  await session.updateCell(2, 1, 'Cy')
  expect(api.updates).toEqual([])

  await session.saveDraft()
  expect(api.inserts).toEqual([{ name: 'Cy' }])
  expect(session.draft).toBeNull()
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
