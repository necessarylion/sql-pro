// Checks how the Session loads and changes indexes and foreign keys, with a fake backend.
import { expect, test } from 'bun:test'
import type { DatabaseApi } from '../src/services/DatabaseApi'
import { Session } from '../src/services/Session'
import { DEFAULT_CONFIGS } from '../src/constants'
import type { NewForeignKey, NewIndex, TableKeys, TablePage, TableRef } from '../src/types'
import { FakePrompt } from './FakePrompt'

const ORDERS: TableRef = { schema: null, name: 'orders', kind: 'table' }
const CUSTOMERS: TableRef = { schema: null, name: 'customers', kind: 'table' }
const RECENT: TableRef = { schema: null, name: 'recent', kind: 'view' }

class FakeApi {
  keyLoads: TableRef[] = []
  added: NewIndex[] = []
  addedKeys: NewForeignKey[] = []
  dropped: string[] = []

  async browse(): Promise<TablePage> {
    return { columns: [], rows: [], total: 0 }
  }

  async tableKeys(_id: string, table: TableRef): Promise<TableKeys> {
    this.keyLoads.push(table)
    return {
      indexes: [{ name: 'PRIMARY', columns: ['id'], unique: true, primary: true }],
      foreignKeys: [],
    }
  }

  async addIndex(_id: string, _table: TableRef, index: NewIndex): Promise<void> {
    this.added.push(index)
  }

  async dropIndex(_id: string, _table: TableRef, name: string): Promise<void> {
    this.dropped.push(name)
  }

  async addForeignKey(_id: string, _table: TableRef, key: NewForeignKey): Promise<void> {
    this.addedKeys.push(key)
  }

  async dropForeignKey(_id: string, _table: TableRef, name: string): Promise<void> {
    this.dropped.push(name)
  }
}

function newSession(): { api: FakeApi; session: Session } {
  const api = new FakeApi()
  const session = new Session(api as unknown as DatabaseApi, new FakePrompt())
  return { api, session }
}

test('keys load with the page only on the Structure view', async function () {
  const { api, session } = newSession()
  await session.selectTable(ORDERS)
  expect(api.keyLoads).toEqual([])
  expect(session.keys).toBeNull()

  session.view = 'structure'
  await session.reload()
  expect(api.keyLoads).toEqual([ORDERS])
  expect(session.keys?.indexes[0].name).toBe('PRIMARY')

  // A view has no keys. Nothing is asked.
  await session.selectTable(RECENT)
  expect(api.keyLoads).toEqual([ORDERS])
  expect(session.keys).toBeNull()
})

test('a new index has a trimmed name, and the keys load again', async function () {
  const { api, session } = newSession()
  session.view = 'structure'
  await session.selectTable(ORDERS)
  await session.addIndex({ name: '  by_customer ', columns: ['customer_id'], unique: false })
  expect(api.added).toEqual([{ name: 'by_customer', columns: ['customer_id'], unique: false }])
  expect(session.status).toBe('Index "by_customer" added')
  expect(api.keyLoads).toEqual([ORDERS, ORDERS])

  // No name: nothing is sent.
  await session.addIndex({ name: ' ', columns: ['id'], unique: true })
  expect(api.added.length).toBe(1)

  await session.dropIndex('by_customer')
  expect(api.dropped).toEqual(['by_customer'])
  expect(session.status).toBe('Index "by_customer" dropped')
})

test('foreign keys are sent with their rules', async function () {
  const { api, session } = newSession()
  session.view = 'structure'
  await session.selectTable(ORDERS)
  await session.addForeignKey({
    name: 'orders_customer ',
    columns: ['customer_id'],
    refTable: CUSTOMERS,
    refColumns: ['id'],
    onDelete: 'CASCADE',
    onUpdate: 'NO ACTION',
  })
  expect(api.addedKeys[0].name).toBe('orders_customer')
  expect(api.addedKeys[0].onDelete).toBe('CASCADE')
  expect(session.status).toBe('Foreign key "orders_customer" added')

  await session.dropForeignKey('orders_customer')
  expect(api.dropped).toEqual(['orders_customer'])
  expect(api.keyLoads.length).toBe(3)
})

test('only SQLite cannot change foreign keys', function () {
  const { session } = newSession()
  session.config = { ...DEFAULT_CONFIGS.mysql }
  expect(session.canChangeForeignKeys).toBe(true)
  session.config = { ...DEFAULT_CONFIGS.sqlite }
  expect(session.canChangeForeignKeys).toBe(false)
})
