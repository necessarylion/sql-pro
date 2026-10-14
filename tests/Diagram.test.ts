// Checks the relation kinds, the automatic layout and the saved positions of the diagram. Run: bun test
import { expect, jest, test } from 'bun:test'
import { DEFAULT_CONFIGS } from '../src/constants'
import { DIAGRAM_LAYOUTS_STORAGE_KEY, DIAGRAM_SAVED_STATUS_MS } from '../src/constants/diagram'
import type { DatabaseApi } from '../src/services/DatabaseApi'
import { DiagramLayout } from '../src/services/DiagramLayout'
import { DiagramLayoutStore } from '../src/services/DiagramLayoutStore'
import { DiagramRelations } from '../src/services/DiagramRelations'
import { ProfileStorage } from '../src/services/ProfileStorage'
import { Session } from '../src/services/Session'
import type { ColumnInfo, Diagram, DiagramTable, Relation } from '../src/types'
import { FakePrompt } from './FakePrompt'

class FakeStorage {
  items: Record<string, string> = {}

  getItem(key: string): string | null {
    return this.items[key] ?? null
  }

  setItem(key: string, value: string): void {
    this.items[key] = value
  }
}

function storeOf(fake: FakeStorage): DiagramLayoutStore {
  return new DiagramLayoutStore(new ProfileStorage(fake as unknown as Storage, ''))
}

/** A table. The names in `primary` are its primary key columns. */
function table(name: string, columns: string[], primary: string[]): DiagramTable {
  const infos: ColumnInfo[] = []
  for (const column of columns) {
    infos.push({
      name: column,
      dataType: 'int',
      nullable: false,
      default: null,
      primaryKey: primary.includes(column),
      extra: '',
    })
  }
  return { table: { schema: null, name, kind: 'table' }, columns: infos }
}

/** A foreign key from `from` to the `id` column of `to`. */
function key(from: string, columns: string[], to: string, unique = false): Relation {
  return {
    name: `fk_${from}_${columns.join('_')}`,
    schema: null,
    table: from,
    columns,
    refSchema: null,
    refTable: to,
    refColumns: ['id'],
    unique,
  }
}

function kinds(diagram: Diagram): string[] {
  const list: string[] = []
  for (const edge of DiagramRelations.edges(diagram)) {
    list.push(`${edge.source} -> ${edge.target}: ${edge.kind}`)
  }
  return list
}

const SHOP: Diagram = {
  tables: [
    table('users', ['id'], ['id']),
    table('profiles', ['id', 'user_id'], ['id']),
    table('orders', ['id', 'user_id'], ['id']),
    table('roles', ['id'], ['id']),
    table('user_roles', ['user_id', 'role_id'], ['user_id', 'role_id']),
    table('notes', ['id'], ['id']),
  ],
  relations: [
    key('profiles', ['user_id'], 'users', true),
    key('orders', ['user_id'], 'users'),
    key('user_roles', ['user_id'], 'users'),
    key('user_roles', ['role_id'], 'roles'),
  ],
}

test('a unique key is one to one, a plain key is one to many', function () {
  const edges = DiagramRelations.edges(SHOP)
  expect(edges[0].kind).toBe('one-to-one')
  expect(edges[1].kind).toBe('one-to-many')
  // The line runs from the "one" side to the table with the key, column to column.
  expect(edges[1].source).toBe('users')
  expect(edges[1].sourceColumn).toBe('id')
  expect(edges[1].target).toBe('orders')
  expect(edges[1].targetColumn).toBe('user_id')
})

test('the two keys of a join table are many to many', function () {
  expect(kinds(SHOP).slice(2)).toEqual([
    'users -> user_roles: many-to-many',
    'roles -> user_roles: many-to-many',
  ])
})

test('a table is no join table when its keys are not the whole primary key', function () {
  // The primary key is its own id column.
  const ownId: Diagram = {
    tables: [
      table('users', ['id'], ['id']),
      table('roles', ['id'], ['id']),
      table('user_roles', ['id', 'user_id', 'role_id'], ['id']),
    ],
    relations: [key('user_roles', ['user_id'], 'users'), key('user_roles', ['role_id'], 'roles')],
  }
  expect(kinds(ownId)).toEqual([
    'users -> user_roles: one-to-many',
    'roles -> user_roles: one-to-many',
  ])

  // Three keys in the primary key.
  const three: Diagram = {
    tables: [table('users', ['id'], ['id']), table('links', ['a', 'b', 'c'], ['a', 'b', 'c'])],
    relations: [
      key('links', ['a'], 'users'),
      key('links', ['b'], 'users'),
      key('links', ['c'], 'users'),
    ],
  }
  expect(kinds(three)).toEqual(new Array(3).fill('users -> links: one-to-many'))
})

test('a join table may have one more key outside its primary key', function () {
  const diagram: Diagram = {
    tables: [
      table('users', ['id'], ['id']),
      table('roles', ['id'], ['id']),
      table('user_roles', ['user_id', 'role_id', 'added_by'], ['user_id', 'role_id']),
    ],
    relations: [
      key('user_roles', ['user_id'], 'users'),
      key('user_roles', ['role_id'], 'roles'),
      key('user_roles', ['added_by'], 'users'),
    ],
  }
  expect(kinds(diagram)).toEqual([
    'users -> user_roles: many-to-many',
    'roles -> user_roles: many-to-many',
    'users -> user_roles: one-to-many',
  ])
})

test('a table can point at itself, also through a join table', function () {
  const diagram: Diagram = {
    tables: [
      table('users', ['id', 'boss_id'], ['id']),
      table('friends', ['user_id', 'friend_id'], ['user_id', 'friend_id']),
    ],
    relations: [
      key('users', ['boss_id'], 'users'),
      key('friends', ['user_id'], 'users'),
      key('friends', ['friend_id'], 'users'),
    ],
  }
  expect(kinds(diagram)).toEqual([
    'users -> users: one-to-many',
    'users -> friends: many-to-many',
    'users -> friends: many-to-many',
  ])
  const edges = DiagramRelations.edges(diagram)
  expect(new Set([edges[0].id, edges[1].id, edges[2].id]).size).toBe(3)
  // The layout does not fail on the self reference.
  const positions = DiagramLayout.positions(diagram, edges, {})
  expect(Object.keys(positions).sort()).toEqual(['friends', 'users'])
})

test('a key over two columns is one line on its first columns', function () {
  const diagram: Diagram = {
    tables: [
      table('orders', ['shop_id', 'number'], ['shop_id', 'number']),
      table('items', ['id', 'shop_id', 'order_number'], ['id']),
    ],
    relations: [
      {
        ...key('items', ['shop_id', 'order_number'], 'orders'),
        refColumns: ['shop_id', 'number'],
      },
    ],
  }
  const edges = DiagramRelations.edges(diagram)
  expect(edges.length).toBe(1)
  expect(edges[0].kind).toBe('one-to-many')
  expect(edges[0].sourceColumn).toBe('shop_id')
  expect(edges[0].targetColumn).toBe('shop_id')
  expect(DiagramRelations.foreignColumns(diagram)).toEqual({ items: ['shop_id', 'order_number'] })
})

test('a key to a table that is not in the diagram is left out', function () {
  const diagram: Diagram = {
    tables: [table('orders', ['id', 'user_id'], ['id'])],
    relations: [key('orders', ['user_id'], 'users')],
  }
  expect(DiagramRelations.edges(diagram)).toEqual([])
})

test('the schema is part of the table id', function () {
  expect(DiagramRelations.tableId(null, 'users')).toBe('users')
  expect(DiagramRelations.tableId('audit', 'users')).toBe('audit.users')
  const diagram: Diagram = {
    tables: [
      {
        ...table('users', ['id'], ['id']),
        table: { schema: 'public', name: 'users', kind: 'table' },
      },
      {
        ...table('logs', ['id', 'user_id'], ['id']),
        table: { schema: 'audit', name: 'logs', kind: 'table' },
      },
    ],
    relations: [{ ...key('logs', ['user_id'], 'users'), schema: 'audit', refSchema: 'public' }],
  }
  expect(kinds(diagram)).toEqual(['public.users -> audit.logs: one-to-many'])
})

test('every table gets a place, and the "one" side is left of the table with the key', function () {
  const positions = DiagramLayout.positions(SHOP, DiagramRelations.edges(SHOP), {})
  expect(Object.keys(positions).length).toBe(SHOP.tables.length)
  expect(positions.users.x).toBeLessThan(positions.orders.x)
  // A table with no relation is put below the others.
  expect(positions.notes.y).toBeGreaterThan(positions.users.y)
  // No two boxes share a place.
  const seen = new Set<string>()
  for (const position of Object.values(positions)) {
    seen.add(`${position.x}:${position.y}`)
  }
  expect(seen.size).toBe(SHOP.tables.length)
})

test('a saved position wins over the automatic one', function () {
  const edges = DiagramRelations.edges(SHOP)
  const automatic = DiagramLayout.positions(SHOP, edges, {})
  const positions = DiagramLayout.positions(SHOP, edges, { orders: { x: 900, y: -50 } })
  expect(positions.orders).toEqual({ x: 900, y: -50 })
  expect(positions.users).toEqual(automatic.users)
})

test('moved boxes are saved per connection and database, and reset forgets them', function () {
  const fake = new FakeStorage()
  const store = storeOf(fake)
  const shop = store.key({ ...DEFAULT_CONFIGS.mysql, database: 'shop' })
  const blog = store.key({ ...DEFAULT_CONFIGS.mysql, database: 'blog' })
  expect(shop).toBe('mysql:root@127.0.0.1:3306/shop')
  expect(shop).not.toBe(blog)
  expect(store.positions(shop)).toEqual({})

  store.move(shop, 'users', { x: 10, y: 20 })
  store.move(shop, 'orders', { x: 30, y: 40 })
  store.move(shop, 'users', { x: 15, y: 25 })
  store.move(blog, 'posts', { x: 1, y: 2 })

  // A new instance reads them back.
  const again = storeOf(fake)
  expect(again.positions(shop)).toEqual({ users: { x: 15, y: 25 }, orders: { x: 30, y: 40 } })

  again.reset(shop)
  expect(again.positions(shop)).toEqual({})
  expect(storeOf(fake).positions(shop)).toEqual({})
  expect(storeOf(fake).positions(blog)).toEqual({ posts: { x: 1, y: 2 } })
})

test('broken saved positions are ignored', function () {
  for (const bad of ['{not json', 'null', '5', '{"a":null}', '{"a":{"users":{"x":"1","y":2}}}']) {
    const fake = new FakeStorage()
    fake.items[DIAGRAM_LAYOUTS_STORAGE_KEY] = bad
    expect(storeOf(fake).positions('a')).toEqual({})
  }
  const fake = new FakeStorage()
  fake.items[DIAGRAM_LAYOUTS_STORAGE_KEY] = '{"a":{"users":{"x":1,"y":2},"orders":[]}}'
  expect(storeOf(fake).positions('a')).toEqual({ users: { x: 1, y: 2 } })
})

class FakeApi {
  broken = false
  loads = 0

  async diagram(): Promise<Diagram> {
    this.loads += 1
    if (this.broken) {
      throw 'no database selected'
    }
    return SHOP
  }

  async saveImage(): Promise<void> {}

  async browse(): Promise<{ columns: never[]; rows: never[]; total: number }> {
    return { columns: [], rows: [], total: 0 }
  }
}

test('the session loads the diagram on refresh and leaves the view on a table click', async function () {
  const api = new FakeApi()
  const session = new Session(api as unknown as DatabaseApi, new FakePrompt())
  session.view = 'diagram'
  await session.refresh()
  expect(session.diagram).toEqual(SHOP)
  expect(api.loads).toBe(1)

  // A failed load does not keep the old diagram.
  api.broken = true
  expect(await session.run(session.refresh())).toBe(false)
  expect(session.error).toBe('no database selected')
  expect(session.diagram).toBeNull()

  await session.selectTable(SHOP.tables[0].table)
  expect(session.view).toBe('content')
})

test('the "Diagram saved" status goes away after a few seconds', async function () {
  jest.useFakeTimers()
  try {
    const session = new Session(new FakeApi() as unknown as DatabaseApi, new FakePrompt())
    await session.saveImage('/tmp/shop.png', [1, 2])
    expect(session.status).toBe('Diagram saved to /tmp/shop.png')
    jest.advanceTimersByTime(DIAGRAM_SAVED_STATUS_MS - 1000)
    expect(session.status).toBe('Diagram saved to /tmp/shop.png')

    // A second export starts the time again. The first timer does not cut it short.
    await session.saveImage('/tmp/shop.png', [1, 2])
    jest.advanceTimersByTime(DIAGRAM_SAVED_STATUS_MS - 1)
    expect(session.status).toBe('Diagram saved to /tmp/shop.png')
    jest.advanceTimersByTime(1)
    expect(session.status).toBe('')

    // A newer status of another action is not removed.
    await session.saveImage('/tmp/shop.png', [1, 2])
    session.status = 'Table "users" truncated'
    jest.advanceTimersByTime(DIAGRAM_SAVED_STATUS_MS)
    expect(session.status).toBe('Table "users" truncated')
  } finally {
    jest.useRealTimers()
  }
})

test('a clicked table is related to the tables it points to and the tables that point to it', function () {
  const edges = DiagramRelations.edges(SHOP)
  expect([...DiagramRelations.related(edges, 'users')].sort()).toEqual([
    'orders',
    'profiles',
    'user_roles',
    'users',
  ])
  expect([...DiagramRelations.related(edges, 'user_roles')].sort()).toEqual([
    'roles',
    'user_roles',
    'users',
  ])
  expect([...DiagramRelations.related(edges, 'notes')]).toEqual(['notes'])
})
