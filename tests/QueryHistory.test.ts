// Checks the history of run SQL. Run: bun test
import { expect, test } from 'bun:test'
import { QUERY_HISTORY_MAX_LENGTH, QUERY_HISTORY_STORAGE_KEY } from '../src/constants/history'
import { ProfileStorage } from '../src/services/ProfileStorage'
import { QueryHistory } from '../src/services/QueryHistory'

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

test('add puts the newest first, with a date and the connection', function () {
  const history = newHistory()
  history.add('SELECT 1', 'Shop')
  history.add('  SELECT 2  ', 'Blog')

  expect(history.entries.length).toBe(2)
  expect(history.entries[0].sql).toBe('SELECT 2')
  expect(history.entries[0].connection).toBe('Blog')
  expect(history.entries[1].sql).toBe('SELECT 1')
  expect(history.entries[0].id).not.toBe(history.entries[1].id)
  // An ISO date gives the same text back.
  expect(new Date(history.entries[0].ranAt).toISOString()).toBe(history.entries[0].ranAt)
})

test('empty SQL is ignored', function () {
  const history = newHistory()
  history.add('', 'Shop')
  history.add('  \n ', 'Shop')
  expect(history.entries).toEqual([])
})

test('the list is cut to the max length and keeps the newest', function () {
  const history = newHistory()
  for (let index = 0; index <= QUERY_HISTORY_MAX_LENGTH; index++) {
    history.add(`SELECT ${index}`, 'Shop')
  }
  expect(history.entries.length).toBe(QUERY_HISTORY_MAX_LENGTH)
  expect(history.entries[0].sql).toBe(`SELECT ${QUERY_HISTORY_MAX_LENGTH}`)
  expect(history.entries[QUERY_HISTORY_MAX_LENGTH - 1].sql).toBe('SELECT 1')
})

test('remove takes out one entry', function () {
  const history = newHistory()
  history.add('SELECT 1', 'Shop')
  history.add('SELECT 2', 'Shop')
  history.remove(history.entries[0].id)
  expect(history.entries.length).toBe(1)
  expect(history.entries[0].sql).toBe('SELECT 1')
})

test('clear removes everything', function () {
  const fake = new FakeStorage()
  const history = new QueryHistory(storageOf(fake))
  history.add('SELECT 1', 'Shop')
  history.clear()
  expect(history.entries).toEqual([])
  expect(new QueryHistory(storageOf(fake)).entries).toEqual([])
})

test('a new instance reads the saved list back', function () {
  const fake = new FakeStorage()
  const history = new QueryHistory(storageOf(fake))
  history.add('SELECT 1', 'Shop')
  history.add('SELECT 2', 'Shop')
  history.remove(history.entries[0].id)
  expect(new QueryHistory(storageOf(fake)).entries).toEqual(history.entries)
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
