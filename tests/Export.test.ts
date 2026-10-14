// A check of the export logic of the Session, with a fake backend. Run: bun test
import { expect, test } from 'bun:test'
import type { DatabaseApi } from '../src/services/DatabaseApi'
import { Session } from '../src/services/Session'
import { FakePrompt } from './FakePrompt'
import type { ExportRequest, Row, RowsExport, TableRef } from '../src/types'

class FakeApi {
  exports: { id: string; request: ExportRequest }[] = []
  rowExports: { id: string; request: RowsExport }[] = []

  async exportRows(id: string, request: RowsExport): Promise<void> {
    this.rowExports.push({ id, request })
  }

  async exportTables(id: string, request: ExportRequest): Promise<void> {
    if (request.path === '/nowhere/x.csv') {
      throw 'No such file or directory'
    }
    this.exports.push({ id, request })
  }
}

const USERS: TableRef = { schema: null, name: 'users', kind: 'table' }
const ORDERS: TableRef = { schema: null, name: 'orders', kind: 'table' }

function request(tables: TableRef[], path: string): ExportRequest {
  return { tables, format: 'csv', structure: false, data: true, columns: ['id'], path }
}

test('export sends the tab id and the request, then says where the file is', async function () {
  const api = new FakeApi()
  const session = new Session(api as unknown as DatabaseApi, new FakePrompt())

  expect(await session.run(session.exportTables(request([USERS], '/tmp/users.csv')))).toBe(true)
  expect(api.exports).toEqual([{ id: session.id, request: request([USERS], '/tmp/users.csv') }])
  expect(session.status).toBe('Exported "users" to /tmp/users.csv')

  await session.run(session.exportTables(request([USERS, ORDERS], '/tmp/out')))
  expect(session.status).toBe('Exported 2 tables to /tmp/out')

  expect(await session.run(session.exportTables(request([USERS], '/nowhere/x.csv')))).toBe(false)
  expect(session.error).toBe('No such file or directory')
})

test('the export file name is the table, or the database for many tables', function () {
  const session = new Session(new FakeApi() as unknown as DatabaseApi, new FakePrompt())
  expect(session.exportName([USERS], 'csv')).toBe('users.csv')
  expect(session.exportName([USERS, ORDERS], 'sql')).toBe('export.sql')

  session.config = {
    driver: 'mysql',
    host: 'db',
    port: 1,
    user: 'me',
    password: '',
    database: 'shop',
    ssh: null,
  }
  expect(session.exportName([USERS, ORDERS], 'sql')).toBe('shop.sql')
})

test('picked rows keep the grid order and skip rows past the end', function () {
  const session = new Session(new FakeApi() as unknown as DatabaseApi, new FakePrompt())
  const rows: Row[] = [['1'], ['2'], ['3']]
  expect(session.pickRows(rows, [])).toEqual(rows)
  // Row 5 is a draft new row in the content view. It is not saved, so it is skipped.
  expect(session.pickRows(rows, [2, 0, 5])).toEqual([['1'], ['3']])
})

test('row export names the table, or uses the file name for a query result', async function () {
  const api = new FakeApi()
  const session = new Session(api as unknown as DatabaseApi, new FakePrompt())
  expect(session.rowsName(USERS, 'csv')).toBe('users.csv')
  expect(session.rowsName(null, 'sql')).toBe('query_result.sql')

  await session.exportRows('sql', USERS, ['id'], [['1']], '/tmp/x.sql')
  expect(api.rowExports[0]).toEqual({
    id: session.id,
    request: { format: 'sql', table: USERS, columns: ['id'], rows: [['1']], path: '/tmp/x.sql' },
  })
  expect(session.status).toBe('Exported 1 row to /tmp/x.sql')

  await session.exportRows('sql', null, ['id'], [['1'], ['2']], '/tmp/old.users.sql')
  expect(api.rowExports[1].request.table.name).toBe('old.users')
  expect(session.status).toBe('Exported 2 rows to /tmp/old.users.sql')
})
