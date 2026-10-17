// Checks the saved (named) queries. Run: bun test
import { expect, test } from 'bun:test'
import { DEFAULT_CONFIGS } from '../src/constants'
import { SAVED_QUERIES_STORAGE_KEY } from '../src/constants/history'
import { ProfileStorage } from '../src/services/ProfileStorage'
import { SavedQueries } from '../src/services/SavedQueries'

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

function names(saved: SavedQueries, key: string): string[] {
  const result: string[] = []
  for (const query of saved.list(key)) {
    result.push(query.name)
  }
  return result
}

test('save keeps the list sorted by name, one list per key', function () {
  const saved = new SavedQueries(storageOf(new FakeStorage()))
  expect(saved.save('a', ' Users ', ' SELECT * FROM users ')).toBe(true)
  expect(saved.save('a', 'Orders', 'SELECT * FROM orders')).toBe(true)
  saved.save('b', 'Other', 'SELECT 1')

  expect(names(saved, 'a')).toEqual(['Orders', 'Users'])
  expect(saved.list('a')[1].sql).toBe('SELECT * FROM users')
  expect(names(saved, 'b')).toEqual(['Other'])
  expect(saved.list('c')).toEqual([])
  expect(saved.has('a', 'Users')).toBe(true)
  expect(saved.has('b', 'Users')).toBe(false)
})

test('save with an existing name replaces the query', function () {
  const saved = new SavedQueries(storageOf(new FakeStorage()))
  saved.save('a', 'Q', 'SELECT 1')
  saved.save('a', 'Q', 'SELECT 2')
  expect(saved.list('a').length).toBe(1)
  expect(saved.list('a')[0].sql).toBe('SELECT 2')
})

test('an empty name or empty SQL is not saved', function () {
  const saved = new SavedQueries(storageOf(new FakeStorage()))
  expect(saved.save('a', '  ', 'SELECT 1')).toBe(false)
  expect(saved.save('a', 'Q', ' \n ')).toBe(false)
  expect(saved.list('a')).toEqual([])
})

test('rename keeps the SQL and refuses a name that is taken or empty', function () {
  const saved = new SavedQueries(storageOf(new FakeStorage()))
  saved.save('a', 'One', 'SELECT 1')
  saved.save('a', 'Two', 'SELECT 2')

  expect(saved.rename('a', 'One', 'Two')).toBe(false)
  expect(saved.rename('a', 'One', ' ')).toBe(false)
  expect(saved.rename('a', 'Missing', 'New')).toBe(false)
  expect(saved.rename('a', 'One', 'One')).toBe(true)
  expect(saved.rename('a', 'One', ' Zero ')).toBe(true)

  expect(names(saved, 'a')).toEqual(['Two', 'Zero'])
  expect(saved.list('a')[1].sql).toBe('SELECT 1')
})

test('remove takes out one query', function () {
  const saved = new SavedQueries(storageOf(new FakeStorage()))
  saved.save('a', 'One', 'SELECT 1')
  saved.save('a', 'Two', 'SELECT 2')
  saved.remove('a', 'One')
  expect(names(saved, 'a')).toEqual(['Two'])
  saved.remove('a', 'Two')
  expect(saved.list('a')).toEqual([])
})

test('a new instance reads the saved lists back', function () {
  const fake = new FakeStorage()
  const saved = new SavedQueries(storageOf(fake))
  saved.save('a', 'One', 'SELECT 1')
  saved.save('b', 'Two', 'SELECT 2')
  const again = new SavedQueries(storageOf(fake))
  expect(again.list('a')).toEqual(saved.list('a'))
  expect(again.list('b')).toEqual(saved.list('b'))
})

test('broken saved data gives empty lists', function () {
  const fake = new FakeStorage()
  fake.items[SAVED_QUERIES_STORAGE_KEY] = '{not json'
  expect(new SavedQueries(storageOf(fake)).list('a')).toEqual([])

  fake.items[SAVED_QUERIES_STORAGE_KEY] = 'null'
  expect(new SavedQueries(storageOf(fake)).list('a')).toEqual([])

  // Entries with a wrong shape are left out.
  fake.items[SAVED_QUERIES_STORAGE_KEY] = '{"a":[null,5,{"name":"x","sql":7}],"b":"text"}'
  const saved = new SavedQueries(storageOf(fake))
  expect(saved.list('a')).toEqual([])
  expect(saved.list('b')).toEqual([])
})

test('key names the server, or the file for SQLite', function () {
  const saved = new SavedQueries(storageOf(new FakeStorage()))
  expect(saved.key(null)).toBe('')
  expect(saved.key({ ...DEFAULT_CONFIGS.mysql, database: 'shop' })).toBe(
    'mysql:root@127.0.0.1:3306',
  )
  // Another database on the same server shares the list.
  expect(saved.key({ ...DEFAULT_CONFIGS.mysql, database: 'blog' })).toBe(
    saved.key({ ...DEFAULT_CONFIGS.mysql, database: 'shop' }),
  )
  expect(saved.key({ ...DEFAULT_CONFIGS.sqlite, database: '/tmp/a.db' })).toBe('sqlite:/tmp/a.db')
})
