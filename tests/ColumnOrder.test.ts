// Checks the display order of grid columns and how it is saved per table. Run: bun test
import { expect, test } from 'bun:test'
import { COLUMN_ORDER_STORAGE_KEY } from '../src/constants/columns'
import { ColumnOrder } from '../src/services/ColumnOrder'
import { ColumnOrderStore } from '../src/services/ColumnOrderStore'
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

test('columns move before or after another column, and reset goes back', function () {
  const order = new ColumnOrder()
  expect(order.shown(4)).toEqual([0, 1, 2, 3])
  // Column 3 goes next to column 0, so the two can be compared.
  order.move(4, 3, 0, true)
  expect(order.shown(4)).toEqual([0, 3, 1, 2])
  order.move(4, 0, 2, true)
  expect(order.shown(4)).toEqual([3, 1, 2, 0])
  order.move(4, 0, 3, false)
  expect(order.shown(4)).toEqual([0, 3, 1, 2])
  // A drop on itself changes nothing.
  order.move(4, 1, 1, true)
  expect(order.shown(4)).toEqual([0, 3, 1, 2])
  // Another column count shows the data order.
  expect(order.shown(3)).toEqual([0, 1, 2])
  order.reset()
  expect(order.shown(4)).toEqual([0, 1, 2, 3])
})

test('a saved order of names comes first, drops gone names and adds new columns at the end', function () {
  const order = new ColumnOrder()
  order.restore(['id', 'name', 'email', 'added'], ['email', 'gone', 'id'])
  expect(order.shown(4)).toEqual([2, 0, 1, 3])
})

test('the store saves names per table, removes the data order and survives bad JSON', function () {
  const fake = new FakeStorage()
  const store = new ColumnOrderStore(new ProfileStorage(fake as unknown as Storage, 'dev'))
  store.save('t', ['b', 'a'], ['a', 'b'])
  expect(
    new ColumnOrderStore(new ProfileStorage(fake as unknown as Storage, 'dev')).load('t'),
  ).toEqual(['b', 'a'])
  expect(fake.items[`${COLUMN_ORDER_STORAGE_KEY}.dev`]).toBe('{"t":["b","a"]}')
  store.save('t', ['a', 'b'], ['a', 'b'])
  expect(store.load('t')).toEqual([])
  fake.items[`${COLUMN_ORDER_STORAGE_KEY}.dev`] = '{broken'
  expect(
    new ColumnOrderStore(new ProfileStorage(fake as unknown as Storage, 'dev')).load('t'),
  ).toEqual([])
})
