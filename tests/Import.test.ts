// A check of the CSV column matching and of the import logic of the Session. Run: bun test
import { expect, test } from 'bun:test'
import { CsvMapping } from '../src/services/CsvMapping'
import type { DatabaseApi } from '../src/services/DatabaseApi'
import { Session } from '../src/services/Session'
import { FakePrompt } from './FakePrompt'
import type { CsvImport, TablePage, TableRef } from '../src/types'

const COLUMNS = ['id', 'name', 'note']
const USERS: TableRef = { schema: null, name: 'users', kind: 'table' }

test('with a header, columns match fields by name, case does not matter', function () {
  const mapping = new CsvMapping()
  expect(mapping.guess(COLUMNS, ['Note', ' ID ', 'other'], true)).toEqual([1, null, 0])
  expect(mapping.fieldNames(['Note', '', 'other'], true)).toEqual(['Note', 'Field 2', 'other'])
})

test('without a header, columns match fields by position', function () {
  const mapping = new CsvMapping()
  expect(mapping.guess(COLUMNS, ['1', 'Ann'], false)).toEqual([0, 1, null])
  expect(mapping.fieldNames(['1', 'Ann'], false)).toEqual(['Field 1', 'Field 2'])
})

test('the header row is not counted, and skipped columns are not sent', function () {
  const mapping = new CsvMapping()
  expect(mapping.rowCount(3, true)).toBe(2)
  expect(mapping.rowCount(3, false)).toBe(3)
  expect(mapping.rowCount(0, true)).toBe(0)
  expect(mapping.chosen(COLUMNS, [2, null, 0])).toEqual([
    { name: 'id', field: 2 },
    { name: 'note', field: 0 },
  ])
})

class FakeApi {
  csv: CsvImport[] = []
  sql: string[] = []
  tablesLoaded = 0
  pagesLoaded = 0

  async browse(): Promise<TablePage> {
    this.pagesLoaded += 1
    return { columns: [], rows: [], total: 0 }
  }

  async importCsv(_id: string, request: CsvImport): Promise<number> {
    this.csv.push(request)
    return 2
  }

  async importSql(_id: string, path: string): Promise<number> {
    if (path === '/bad.sql') {
      throw 'Statement at line 2: syntax error'
    }
    this.sql.push(path)
    return 3
  }

  async listTables(): Promise<TableRef[]> {
    this.tablesLoaded += 1
    return [USERS]
  }
}

function csvRequest(): CsvImport {
  return {
    table: USERS,
    path: '/tmp/users.csv',
    header: true,
    emptyIsNull: true,
    columns: [{ name: 'id', field: 0 }],
  }
}

test('CSV import sends the request and says how many rows went in', async function () {
  const api = new FakeApi()
  const session = new Session(api as unknown as DatabaseApi, new FakePrompt())
  expect(await session.run(session.importCsv(csvRequest()))).toBe(true)
  expect(api.csv).toEqual([csvRequest()])
  expect(session.status).toBe('Imported 2 rows into "users"')
})

test('SQL import runs the file, then loads the tables again', async function () {
  const api = new FakeApi()
  const session = new Session(api as unknown as DatabaseApi, new FakePrompt())
  expect(await session.run(session.importSql('/tmp/shop.sql'))).toBe(true)
  expect(session.status).toBe('Ran 3 statements from /tmp/shop.sql')
  expect(api.tablesLoaded).toBe(1)
  expect(session.tables).toEqual([USERS])

  expect(await session.run(session.importSql('/bad.sql'))).toBe(false)
  expect(session.error).toBe('Statement at line 2: syntax error')
})

test('CSV import into the open table asks about unsaved changes, then loads the rows', async function () {
  const api = new FakeApi()
  const prompt = new FakePrompt()
  const session = new Session(api as unknown as DatabaseApi, prompt)
  await session.selectTable(USERS)
  expect(api.pagesLoaded).toBe(1)
  session.addRow()

  prompt.answer = false
  expect(await session.run(session.importCsv(csvRequest()))).toBe(true)
  expect(prompt.questions).toEqual(['You have 1 unsaved change in "users". Discard it?'])
  expect(api.csv).toEqual([])
  expect(session.pendingCount).toBe(1)

  prompt.answer = true
  await session.run(session.importCsv(csvRequest()))
  expect(api.csv.length).toBe(1)
  expect(session.pendingCount).toBe(0)
  expect(api.pagesLoaded).toBe(2)
})
