// Checks pinning and unpinning sidebar tables and that pins are saved per database. Run: bun test
import { expect, test } from 'bun:test'
import { DEFAULT_CONFIGS } from '../src/constants'
import { PINNED_TABLES_STORAGE_KEY } from '../src/constants/pins'
import { PinnedTables } from '../src/services/PinnedTables'
import { ProfileStorage } from '../src/services/ProfileStorage'

class FakeStorage {
  items: Record<string, string> = {}

  getItem(key: string): string | null {
    return this.items[key] ?? null
  }

  setItem(key: string, value: string): void {
    this.items[key] = value
  }
}

function pinsOf(fake: FakeStorage): PinnedTables {
  return new PinnedTables(new ProfileStorage(fake as unknown as Storage, ''))
}

const SHOP = PinnedTables.prototype.key({ ...DEFAULT_CONFIGS.mysql, database: 'shop' })
const BLOG = PinnedTables.prototype.key({ ...DEFAULT_CONFIGS.mysql, database: 'blog' })

test('pin adds a table once, in pin order', function () {
  const pins = pinsOf(new FakeStorage())
  pins.pin(SHOP, 'orders')
  pins.pin(SHOP, 'users')
  pins.pin(SHOP, 'orders')
  expect(pins.list(SHOP)).toEqual(['orders', 'users'])
  expect(pins.has(SHOP, 'users')).toBe(true)
})

test('unpin removes only that table', function () {
  const pins = pinsOf(new FakeStorage())
  pins.pin(SHOP, 'orders')
  pins.pin(SHOP, 'users')
  pins.unpin(SHOP, 'orders')
  expect(pins.list(SHOP)).toEqual(['users'])
  pins.unpin(SHOP, 'users')
  expect(pins.list(SHOP)).toEqual([])
})

test('pins are saved and read back', function () {
  const fake = new FakeStorage()
  pinsOf(fake).pin(SHOP, 'orders')
  expect(fake.items[PINNED_TABLES_STORAGE_KEY]).toBeDefined()
  expect(pinsOf(fake).list(SHOP)).toEqual(['orders'])
})

test('each database has its own pins', function () {
  const pins = pinsOf(new FakeStorage())
  pins.pin(SHOP, 'orders')
  pins.pin(BLOG, 'posts')
  expect(pins.list(SHOP)).toEqual(['orders'])
  expect(pins.list(BLOG)).toEqual(['posts'])
  expect(SHOP).not.toBe(BLOG)
})

test('broken saved data starts with no pins', function () {
  const fake = new FakeStorage()
  fake.items[PINNED_TABLES_STORAGE_KEY] = '{"a": [1, "x"], "b": "nope"'
  expect(pinsOf(fake).list('a')).toEqual([])
  fake.items[PINNED_TABLES_STORAGE_KEY] = '{"a": [1, "x"], "b": "nope"}'
  expect(pinsOf(fake).list('a')).toEqual(['x'])
  expect(pinsOf(fake).list('b')).toEqual([])
})
