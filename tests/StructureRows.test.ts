// Checks the rows of the Structure grid. Run: bun test
import { expect, test } from 'bun:test'
import { StructureRows } from '../src/services/StructureRows'
import type { ColumnInfo, ForeignKey, TableKeys, TableRef } from '../src/types'

const structure = new StructureRows()

function column(name: string, more: Partial<ColumnInfo> = {}): ColumnInfo {
  return {
    name,
    dataType: 'int',
    nullable: false,
    default: null,
    primaryKey: false,
    extra: '',
    ...more,
  }
}

function key(
  columns: string[],
  refTable: string,
  refColumns: string[],
  refSchema: string | null = null,
): ForeignKey {
  return {
    name: 'fk',
    columns,
    refSchema,
    refTable,
    refColumns,
    onDelete: 'CASCADE',
    onUpdate: 'NO ACTION',
  }
}

const COLUMNS = [
  column('id', { primaryKey: true, extra: 'auto_increment' }),
  column('material_id', { nullable: true }),
  column('name', { dataType: 'varchar(20)', default: 'x' }),
]

const KEYS: TableKeys = {
  indexes: [],
  foreignKeys: [key(['material_id'], 'ads_materials', ['id'])],
  columns: [
    { name: 'id', charset: null, collation: null, comment: null },
    { name: 'material_id', charset: null, collation: null, comment: 'The material' },
    { name: 'name', charset: 'utf8mb4', collation: 'utf8mb4_general_ci', comment: null },
  ],
}

test('the cells come in the header order, with the details and the foreign key', function () {
  expect(structure.rows(COLUMNS, KEYS)).toEqual([
    ['id', 'int', null, null, 'NO', 'PRI', null, 'auto_increment', null, null],
    ['material_id', 'int', null, null, 'YES', '', null, '', 'ads_materials(id)', 'The material'],
    ['name', 'varchar(20)', 'utf8mb4', 'utf8mb4_general_ci', 'NO', '', 'x', '', null, null],
  ])
})

test('with no keys loaded yet (or a view, or SQLite details) the new cells are NULL', function () {
  const [row] = structure.rows([COLUMNS[1]], null)
  expect(row).toEqual(['material_id', 'int', null, null, 'YES', '', null, '', null, null])
})

test('a composite key shows each column its own part; two keys are joined', function () {
  const keys: TableKeys = {
    indexes: [],
    foreignKeys: [key(['a', 'b'], 'parent', ['x', 'y'], 'other'), key(['b'], 'more', ['id'])],
    columns: [],
  }
  expect(structure.reference(keys, 'a')).toBe('other.parent(x)')
  expect(structure.reference(keys, 'b')).toBe('other.parent(y), more(id)')
  expect(structure.reference(keys, 'c')).toBeNull()
})

test('the arrow target is the listed table, in the schema of the open table by default', function () {
  const open: TableRef = { schema: 'public', name: 'ads', kind: 'table' }
  const materials: TableRef = { schema: 'public', name: 'ads_materials', kind: 'table' }
  const elsewhere: TableRef = { schema: 'other', name: 'ads_materials', kind: 'table' }
  const tables = [elsewhere, materials]
  expect(structure.target(KEYS, 'material_id', open, tables)).toBe(materials)
  const keys = { ...KEYS, foreignKeys: [key(['material_id'], 'ads_materials', ['id'], 'other')] }
  expect(structure.target(keys, 'material_id', open, tables)).toBe(elsewhere)
  expect(structure.target(KEYS, 'name', open, tables)).toBeNull()
  expect(structure.target(KEYS, 'material_id', open, [])).toBeNull()
})
