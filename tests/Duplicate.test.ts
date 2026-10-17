// Checks "Duplicate" of rows in the Content view, with a fake backend. Run: bun test
import { expect, test } from 'bun:test'
import type { DatabaseApi } from '../src/services/DatabaseApi'
import { Session } from '../src/services/Session'
import type { ColumnInfo, ConnectionConfig, Row, TablePage, TableRef } from '../src/types'
import { FakePrompt } from './FakePrompt'

const ITEMS: TableRef = { schema: null, name: 'items', kind: 'table' }

function column(name: string, more: Partial<ColumnInfo> = {}): ColumnInfo {
  return {
    name,
    dataType: 'text',
    nullable: true,
    default: null,
    primaryKey: false,
    extra: '',
    ...more,
  }
}

class FakeApi {
  constructor(
    private columns: ColumnInfo[],
    private rows: Row[],
  ) {}

  async browse(): Promise<TablePage> {
    return { columns: this.columns, rows: structuredClone(this.rows), total: this.rows.length }
  }
}

async function open(columns: ColumnInfo[], rows: Row[], driver = 'mysql'): Promise<Session> {
  const session = new Session(
    new FakeApi(columns, rows) as unknown as DatabaseApi,
    new FakePrompt(),
  )
  session.config = { driver } as ConnectionConfig
  await session.selectTable(ITEMS)
  return session
}

test('copies the shown values, also unsaved edits, as new rows in grid order', async function () {
  const session = await open(
    [column('code', { primaryKey: true }), column('name'), column('note')],
    [
      ['a', 'Ann', null],
      ['b', 'Bob', 'x'],
    ],
  )
  session.updateCell(1, 1, 'Bobby')
  // Given in click order. The copies come in grid order.
  expect(session.duplicateRows([1, 0])).toEqual([2, 3])
  // A primary key the database does not fill is copied. The user changes it before Save.
  expect(session.newRows).toEqual([
    ['a', 'Ann', null],
    ['b', 'Bobby', 'x'],
  ])
  expect(session.page?.rows.length).toBe(2)

  // A new row can be duplicated too.
  expect(session.duplicateRows([3])).toEqual([4])
  expect(session.newRows[2]).toEqual(['b', 'Bobby', 'x'])
  expect(session.duplicateRows([9])).toEqual([])
})

test('columns the database fills itself stay empty', async function () {
  const session = await open(
    [
      column('id', { primaryKey: true, dataType: 'int', extra: 'auto_increment' }),
      column('uid', { extra: 'identity' }),
      column('serial', { default: "nextval('items_serial_seq'::regclass)" }),
      column('total', { extra: 'STORED GENERATED' }),
      column('made', { default: 'CURRENT_TIMESTAMP', extra: 'DEFAULT_GENERATED' }),
      column('name'),
    ],
    [['1', '2', '3', '4', '2024-01-01 00:00:00', 'Ann']],
  )
  session.duplicateRows([0])
  // A column with only a default (DEFAULT_GENERATED) is copied.
  expect(session.newRows).toEqual([[null, null, null, null, '2024-01-01 00:00:00', 'Ann']])
})

test('SQLite: only a single INTEGER primary key numbers the rows itself', async function () {
  const single = await open(
    [column('id', { primaryKey: true, dataType: 'INTEGER' }), column('name')],
    [['1', 'Ann']],
    'sqlite',
  )
  single.duplicateRows([0])
  expect(single.newRows).toEqual([[null, 'Ann']])

  const composite = await open(
    [
      column('a', { primaryKey: true, dataType: 'INTEGER' }),
      column('b', { primaryKey: true, dataType: 'INTEGER' }),
    ],
    [['1', '2']],
    'sqlite',
  )
  composite.duplicateRows([0])
  expect(composite.newRows).toEqual([['1', '2']])
})
