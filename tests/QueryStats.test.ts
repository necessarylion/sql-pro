// Checks the Query stats list and the Session method, with a fake backend. Run: bun test
import { expect, test } from 'bun:test'
import { DEFAULT_CONFIGS } from '../src/constants'
import type { DatabaseApi } from '../src/services/DatabaseApi'
import { QueryStatList } from '../src/services/QueryStatList'
import { Session } from '../src/services/Session'
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

  async queryStats(_id: string, days: number): Promise<QueryStat[]> {
    this.days.push(days)
    return [FAST, SLOW]
  }
}

async function connected(api: FakeApi): Promise<Session> {
  const session = new Session(api as unknown as DatabaseApi, new FakePrompt())
  await session.connect(DEFAULT_CONFIGS.mysql, 'test')
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

test('the filter looks at the query and the database', function () {
  const list = new QueryStatList()
  list.filter = ' ORDERS '
  expect(list.visible([FAST, SLOW])).toEqual([SLOW])
  list.filter = 'shop'
  expect(list.visible([FAST, SLOW])).toEqual([SLOW])
})

test('durations get a short unit', function () {
  expect(QueryStatList.duration(null)).toBe('')
  expect(QueryStatList.duration(0.4213)).toBe('0.42 ms')
  expect(QueryStatList.duration(12.4)).toBe('12 ms')
  expect(QueryStatList.duration(3500)).toBe('3.5 s')
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

test('refresh in the Query stats view loads them again, and disconnect clears them', async function () {
  const api = new FakeApi()
  const session = await connected(api)
  session.view = 'queryStats'
  await session.refresh()
  expect(api.days).toEqual([7])
  await session.disconnect()
  expect(session.queryStats).toEqual([])
})
