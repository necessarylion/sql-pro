// Checks the Query stats list and the Session method, with a fake backend. Run: bun test
import { expect, test } from 'bun:test'
import { DEFAULT_CONFIGS } from '../src/constants'
import type { DatabaseApi } from '../src/services/DatabaseApi'
import { QueryStatList } from '../src/services/QueryStatList'
import { Session } from '../src/services/Session'
import { ExplainQueryPrompt } from '../src/services/ExplainQueryPrompt'
import { FakePrompt } from './FakePrompt'
import type { QueryStat, TableRef, UserList } from '../src/types'

const SLOW: QueryStat = {
  query: 'SELECT * FROM orders WHERE id = ?',
  database: 'shop',
  runs: 3,
  lastRun: '2026-10-17 10:00:00',
  totalMs: 900,
  averageMs: 300,
  maxMs: 500,
  rows: 3,
}
const FAST: QueryStat = {
  ...SLOW,
  query: 'UPDATE stock SET count = ?',
  database: null,
  runs: 50,
  totalMs: 5,
  maxMs: null,
}

class FakeApi {
  days: number[] = []

  async connect(): Promise<void> {}

  async disconnect(): Promise<void> {}

  async listDatabases(): Promise<string[]> {
    return ['shop']
  }

  async listTables(): Promise<TableRef[]> {
    return []
  }

  async listUsers(): Promise<UserList> {
    return { canManage: false, users: [] }
  }

  async useDatabase(): Promise<void> {}

  async queryStats(_id: string, days: number): Promise<QueryStat[]> {
    this.days.push(days)
    return [FAST, SLOW]
  }
}

async function connected(api: FakeApi): Promise<Session> {
  const session = new Session(api as unknown as DatabaseApi, new FakePrompt())
  await session.connect({ ...DEFAULT_CONFIGS.mysql, database: 'shop' }, 'test')
  return session
}

test('the list starts with the most total time first', function () {
  expect(new QueryStatList().visible([FAST, SLOW])).toEqual([SLOW, FAST])
})

test('a click on a column sorts by it, a second click turns the order around', function () {
  const list = new QueryStatList()
  list.sortBy('runs')
  expect(list.visible([SLOW, FAST])).toEqual([FAST, SLOW])
  list.sortBy('runs')
  expect(list.visible([FAST, SLOW])).toEqual([SLOW, FAST])
  list.sortBy('query')
  expect(list.descending).toBe(false)
  expect(list.visible([FAST, SLOW])).toEqual([SLOW, FAST])
})

test('empty values go last in both orders', function () {
  const list = new QueryStatList()
  list.sortBy('maxMs')
  expect(list.visible([FAST, SLOW])).toEqual([SLOW, FAST])
  list.sortBy('maxMs')
  expect(list.visible([FAST, SLOW])).toEqual([SLOW, FAST])
})

test('the filter looks at the query', function () {
  const list = new QueryStatList()
  list.filter = ' ORDERS '
  expect(list.visible([FAST, SLOW])).toEqual([SLOW])
})

test('every column sorts: numbers by value, the last run by time', function () {
  const nine: QueryStat = { ...SLOW, runs: 9, lastRun: '2026-09-30 23:00:00', rows: 9 }
  const ten: QueryStat = { ...SLOW, runs: 10, lastRun: '2026-10-01 01:00:00', rows: 10 }
  const list = new QueryStatList()
  for (const name of ['Runs', 'Last run', 'Rows']) {
    list.sortByName(name)
    expect(list.descending).toBe(true)
    expect(list.visible([nine, ten])).toEqual([ten, nine])
  }
  for (const name of ['Total', 'Average', 'Max']) {
    list.sortByName(name)
    expect(list.columnName).toBe(name)
  }
  list.sortByName('Query')
  expect(list.column).toBe('query')
})

test('durations get a short unit', function () {
  expect(QueryStatList.duration(null)).toBe('')
  expect(QueryStatList.duration(0.4213)).toBe('0.42 ms')
  expect(QueryStatList.duration(12.4)).toBe('12 ms')
  expect(QueryStatList.duration(3500)).toBe('3.5 s')
})

test('a grid row shows times with a unit, but the sort uses the numbers', function () {
  const slower: QueryStat = { ...FAST, totalMs: 2000 }
  expect(QueryStatList.row(slower)).toEqual([
    'UPDATE stock SET count = ?',
    '50',
    '2026-10-17 10:00:00',
    '2.0 s',
    '300 ms',
    null,
    '3',
  ])
  expect(new QueryStatList().visible([SLOW, slower])).toEqual([slower, SLOW])
})

test('a click on a grid header sorts by that column', function () {
  const list = new QueryStatList()
  expect(list.columnName).toBe('Total')
  list.sortByName('Last run')
  expect(list.column).toBe('lastRun')
  expect(list.columnName).toBe('Last run')
  expect(QueryStatList.columns()[1]).toEqual({ name: 'Runs', type: 'numeric' })
})

test('loadQueryStats sends whole days in bounds', async function () {
  const api = new FakeApi()
  const session = await connected(api)
  await session.loadQueryStats()
  expect(session.queryStats).toEqual([FAST, SLOW])
  session.queryStatsDays = 0
  await session.loadQueryStats()
  session.queryStatsDays = 2.6
  await session.loadQueryStats()
  session.queryStatsDays = 99999
  await session.loadQueryStats()
  expect(api.days).toEqual([7, 1, 3, 3650])
})

test('no database means no stats and no call, a switch loads the stats of the new one', async function () {
  const api = new FakeApi()
  const session = new Session(api as unknown as DatabaseApi, new FakePrompt())
  await session.connect(DEFAULT_CONFIGS.mysql, 'test')
  session.view = 'queryStats'
  await session.loadQueryStats()
  expect(session.queryStats).toEqual([])
  expect(api.days).toEqual([])
  await session.useDatabase('shop')
  expect(api.days).toEqual([7])
  expect(session.queryStats).toEqual([FAST, SLOW])
})

test('refresh in the Query stats view loads them again, and disconnect clears them', async function () {
  const api = new FakeApi()
  const session = await connected(api)
  session.view = 'queryStats'
  await session.refresh()
  expect(api.days).toEqual([7])
  await session.disconnect()
  expect(session.queryStats).toEqual([])
})

test('the explain question holds the kind, the database, the query, the stats and the ask', function () {
  const text = ExplainQueryPrompt.build(SLOW, 'mysql', 'other')
  expect(text).toContain('Explain this query from my MySQL database "shop"')
  expect(text).not.toContain('slow')
  expect(text).toContain('If the query is already fine, say so plainly.')
  expect(text).toContain('SELECT * FROM orders WHERE id = ?')
  expect(text).toContain('put a ? in the place of each real value')
  expect(text).toContain('- Runs: 3')
  expect(text).toContain('- Total time: 900 ms')
  expect(text).toContain('- Average time: 300 ms')
  expect(text).toContain('- Max time: 500 ms')
  expect(text).toContain('- Rows returned (all runs): 3')
  expect(text).toContain('- Last run: 2026-10-17 10:00:00')
  expect(text).toContain('EXPLAIN')
  expect(text).toContain('CREATE INDEX')
  expect(text).toContain('Do not change anything. Only read.')
})

test('the explain question leaves out empty stats and names the placeholders of each server', function () {
  const bare: QueryStat = { ...FAST, lastRun: null, rows: null }
  const text = ExplainQueryPrompt.build(bare, 'postgres', 'shop')
  expect(text).toContain('PostgreSQL database "shop"')
  expect(text).toContain('$1, $2')
  expect(text).not.toContain('Max time')
  expect(text).not.toContain('Rows returned')
  expect(text).not.toContain('Last run')
  expect(ExplainQueryPrompt.build(SLOW, 'mssql', 'shop')).toContain('@P1')
})

test('askAiReadOnly opens the Ask AI view in a new chat, and refuses while the AI is busy', async function () {
  const session = await connected(new FakeApi())
  session.chat.entries.push({ kind: 'user', text: 'old question' })
  await session.askAiReadOnly('explain this')
  expect(session.view).toBe('ask')
  expect(session.readOnlyQuestion).toBe('explain this')
  expect(session.chat.entries).toEqual([])
  // The view has not taken the first one yet.
  await expect(session.askAiReadOnly('again')).rejects.toContain('busy')
  session.readOnlyQuestion = null
  session.chat.phase = 'thinking'
  session.view = 'queryStats'
  await expect(session.askAiReadOnly('again')).rejects.toContain('busy')
  expect(session.view).toBe('queryStats')
  expect(session.readOnlyQuestion).toBeNull()
})
