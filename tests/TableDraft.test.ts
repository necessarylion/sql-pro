// Checks the rules of the "New table" form. Run: bun test
import { expect, test } from 'bun:test'
import { TableDraft } from '../src/services/TableDraft'
import type { ColumnDraft } from '../src/services/TableDraft'

function column(name: string, dataType = 'TEXT'): ColumnDraft {
  return { ...TableDraft.emptyColumn(), name, dataType }
}

test('a new table starts with an id that numbers itself', function () {
  const id = TableDraft.idColumn()
  expect(TableDraft.problem('users', [id])).toBe('')
  expect(TableDraft.columns([id])).toEqual([
    {
      name: 'id',
      dataType: 'INTEGER',
      nullable: false,
      default: null,
      primaryKey: true,
      autoIncrement: true,
    },
  ])
})

test('the name and the type lose their spaces, an empty default is no default', function () {
  const typed = { name: ' age ', dataType: ' INT ', nullable: true, default: '  ' }
  expect(TableDraft.spec(typed)).toEqual({
    name: 'age',
    dataType: 'INT',
    nullable: true,
    default: null,
  })
  // A default is raw SQL. Its spaces can be part of a text, so it is sent as typed.
  expect(TableDraft.spec({ ...typed, default: "' x '" }).default).toBe("' x '")
})

test('a table needs a name, a column, and a name and a type for every column', function () {
  expect(TableDraft.problem('  ', [column('a')])).toBe('The table needs a name')
  expect(TableDraft.problem('t', [])).toBe('The table needs at least one column')
  expect(TableDraft.problem('t', [column(' ')])).toBe('Every column needs a name')
  expect(TableDraft.problem('t', [column('a', ' ')])).toBe('Column "a" needs a type')
  expect(TableDraft.problem('t', [column('a'), column('b')])).toBe('')
})

test('two columns with the same name are refused', function () {
  expect(TableDraft.problem('t', [column('a'), column(' a ')])).toBe('Two columns are named "a"')
})

test('auto increment needs a primary key of one column', function () {
  const id = TableDraft.idColumn()
  const other = { ...column('code'), primaryKey: true }
  const problem = 'Auto increment only works on a column that is the whole primary key'
  expect(TableDraft.problem('t', [id, other])).toBe(problem)
  expect(TableDraft.problem('t', [{ ...id, primaryKey: false }])).toBe(problem)

  // The form fixes itself when a key box changes.
  other.nullable = true
  const columns = [id, other]
  TableDraft.keyChanged(columns)
  expect(id.autoIncrement).toBe(false)
  expect(other.nullable).toBe(false)
  expect(TableDraft.problem('t', columns)).toBe('')
})
