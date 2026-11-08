// Checks the history of run SQL. Run: bun test
import { expect, test } from 'bun:test'
import {
  QUERY_HISTORY_MAX_LENGTH,
  QUERY_HISTORY_STORAGE_KEY,
  QUERY_HISTORY_TOTAL_MAX_LENGTH,
} from '../src/constants/history'
import { ProfileStorage } from '../src/services/ProfileStorage'
import { QueryHistory, type QueryHistoryEntry } from '../src/services/QueryHistory'
import type { ConnectionConfig } from '../src/types'

class FakeStorage {
  items: Record<string, string> = {}

  getItem(key: string): string | null {
    return this.items[key] ?? null
  }

  setItem(key: string, value: string): void {
    this.items[key] = value
  }
}

function storageOf(fake: FakeStorage): ProfileStorage {
  return new ProfileStorage(fake as unknown as Storage, '')
}

function newHistory(): QueryHistory {
  return new QueryHistory(storageOf(new FakeStorage()))
}

const SHOP = 'mysql:127.0.0.1:3306/shop'
const BLOG = 'mysql:127.0.0.1:3306/blog'

function config(database: string, user = 'root'): ConnectionConfig {
  return {
    driver: 'mysql',
    host: '127.0.0.1',
    port: 3306,
    user,
    password: '',
    database,
  } as ConnectionConfig
}

test('add puts the newest first, with a date, the tab title and the database', function () {
  const history = newHistory()
  history.add('SELECT 1', SHOP, 'Shop')
  history.add('  SELECT 2  ', SHOP, 'Local')

  const entries = history.list(SHOP)
  expect(entries.length).toBe(2)
  expect(entries[0].sql).toBe('SELECT 2')
  expect(entries[0].connection).toBe('Local')
  expect(entries[0].database).toBe(SHOP)
  expect(entries[1].sql).toBe('SELECT 1')
  expect(entries[0].id).not.toBe(entries[1].id)
  // An ISO date gives the same text back.
  expect(new Date(entries[0].ranAt).toISOString()).toBe(entries[0].ranAt)
})

test('empty SQL is ignored', function () {
  const history = newHistory()
  history.add('', SHOP, 'Shop')
  history.add('  \n ', SHOP, 'Shop')
  expect(history.entries).toEqual([])
})

test('the key is driver, host, port and database, not the user', function () {
  const history = newHistory()
  expect(history.key(config('shop'))).toBe(SHOP)
  expect(history.key(config('shop', 'admin'))).toBe(SHOP)
  expect(history.key(config('blog'))).toBe(BLOG)
  expect(history.key({ driver: 'sqlite', database: '/data/app.db' } as ConnectionConfig)).toBe(
    'sqlite:/data/app.db',
  )
  expect(history.key(null)).toBe('')
})

test('each database sees only its own entries', function () {
  const history = newHistory()
  history.add('SELECT 1', SHOP, 'Local')
  history.add('SELECT 2', BLOG, 'Local')
  history.add('SELECT 3', SHOP, 'Local')

  expect(history.list(SHOP).map(sqlOf)).toEqual(['SELECT 3', 'SELECT 1'])
  expect(history.list(BLOG).map(sqlOf)).toEqual(['SELECT 2'])
  expect(history.list('mysql:127.0.0.1:3306/other')).toEqual([])
})

test('switching the database of a tab changes the list', function () {
  const history = newHistory()
  const tab = config('shop')
  history.add('SELECT 1', history.key(tab), 'Local')
  tab.database = 'blog'
  expect(history.list(history.key(tab))).toEqual([])
  history.add('SELECT 2', history.key(tab), 'Local')
  expect(history.list(history.key(tab)).map(sqlOf)).toEqual(['SELECT 2'])
  tab.database = 'shop'
  expect(history.list(history.key(tab)).map(sqlOf)).toEqual(['SELECT 1'])
})

test('each database keeps the max length, so a busy one does not push out the others', function () {
  const history = newHistory()
  history.add('SELECT blog', BLOG, 'Local')
  for (let index = 0; index <= QUERY_HISTORY_MAX_LENGTH; index++) {
    history.add(`SELECT ${index}`, SHOP, 'Local')
  }
  const shop = history.list(SHOP)
  expect(shop.length).toBe(QUERY_HISTORY_MAX_LENGTH)
  expect(shop[0].sql).toBe(`SELECT ${QUERY_HISTORY_MAX_LENGTH}`)
  expect(shop[QUERY_HISTORY_MAX_LENGTH - 1].sql).toBe('SELECT 1')
  expect(history.list(BLOG).map(sqlOf)).toEqual(['SELECT blog'])
})

test('all databases together keep the total max length', function () {
  const history = newHistory()
  for (let index = 0; index <= QUERY_HISTORY_TOTAL_MAX_LENGTH; index++) {
    history.add(`SELECT ${index}`, `sqlite:/db${index}`, 'Local')
  }
  expect(history.entries.length).toBe(QUERY_HISTORY_TOTAL_MAX_LENGTH)
  expect(history.list('sqlite:/db0')).toEqual([])
})

test('remove takes out one entry', function () {
  const history = newHistory()
  history.add('SELECT 1', SHOP, 'Shop')
  history.add('SELECT 2', SHOP, 'Shop')
  history.remove(history.list(SHOP)[0].id)
  expect(history.list(SHOP).map(sqlOf)).toEqual(['SELECT 1'])
})

test('clear removes only the entries of one database', function () {
  const fake = new FakeStorage()
  const history = new QueryHistory(storageOf(fake))
  history.add('SELECT 1', SHOP, 'Shop')
  history.add('SELECT 2', BLOG, 'Blog')
  history.clear(SHOP)
  expect(history.list(SHOP)).toEqual([])
  expect(history.list(BLOG).map(sqlOf)).toEqual(['SELECT 2'])
  const again = new QueryHistory(storageOf(fake))
  expect(again.list(SHOP)).toEqual([])
  expect(again.list(BLOG).map(sqlOf)).toEqual(['SELECT 2'])
})

test('a new instance reads the saved list back', function () {
  const fake = new FakeStorage()
  const history = new QueryHistory(storageOf(fake))
  history.add('SELECT 1', SHOP, 'Shop')
  history.add('SELECT 2', BLOG, 'Shop')
  history.remove(history.entries[0].id)
  expect(new QueryHistory(storageOf(fake)).entries).toEqual(history.entries)
})

test('old entries saved without a database are dropped', function () {
  const fake = new FakeStorage()
  fake.items[QUERY_HISTORY_STORAGE_KEY] = JSON.stringify([
    { id: 'a', sql: 'SELECT 1', ranAt: '2026-01-01T00:00:00.000Z', connection: 'Shop' },
    {
      id: 'b',
      sql: 'SELECT 2',
      ranAt: '2026-01-01T00:00:00.000Z',
      connection: 'Shop',
      database: SHOP,
    },
  ])
  const history = new QueryHistory(storageOf(fake))
  expect(history.entries.map(sqlOf)).toEqual(['SELECT 2'])
})

test('broken saved data gives an empty list', function () {
  const fake = new FakeStorage()
  fake.items[QUERY_HISTORY_STORAGE_KEY] = '{not json'
  expect(new QueryHistory(storageOf(fake)).entries).toEqual([])

  fake.items[QUERY_HISTORY_STORAGE_KEY] = '{"sql":"SELECT 1"}'
  expect(new QueryHistory(storageOf(fake)).entries).toEqual([])

  // Entries with a wrong shape are left out.
  fake.items[QUERY_HISTORY_STORAGE_KEY] = '[null, 5, {"id":"a","sql":7}]'
  expect(new QueryHistory(storageOf(fake)).entries).toEqual([])
})

function sqlOf(entry: QueryHistoryEntry): string {
  return entry.sql
}
