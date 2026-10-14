// Checks the table selection of the sidebar: plain click, Cmd-click and Shift-click. Run: bun test
import { expect, test } from 'bun:test'
import { TableSelection } from '../src/services/TableSelection'
import type { TableRef } from '../src/types'

const ORDER = ['a', 'b', 'c', 'd', 'e']

test('a plain click selects nothing but sets where a range starts', function () {
  const selection = new TableSelection()
  selection.start('b')
  expect(selection.names).toEqual([])
  selection.range(ORDER, 'd')
  expect(selection.names).toEqual(['b', 'c', 'd'])
})

test('a range goes up too and keeps its anchor', function () {
  const selection = new TableSelection()
  selection.only('c')
  selection.range(ORDER, 'a')
  expect(selection.names).toEqual(['a', 'b', 'c'])
  selection.range(ORDER, 'e')
  expect(selection.names).toEqual(['c', 'd', 'e'])
})

test('a range with no anchor, or a hidden anchor, selects only the clicked name', function () {
  const selection = new TableSelection()
  selection.range(ORDER, 'd')
  expect(selection.names).toEqual(['d'])
  selection.anchor = 'gone'
  selection.range(ORDER, 'b')
  expect(selection.names).toEqual(['b'])
})

test('toggle adds and removes, and moves the anchor', function () {
  const selection = new TableSelection()
  selection.toggle('a')
  selection.toggle('c')
  expect(selection.names).toEqual(['a', 'c'])
  selection.toggle('a')
  expect(selection.names).toEqual(['c'])
  expect(selection.anchor).toBe('a')
  expect(selection.has('c')).toBe(true)
  selection.clear()
  expect(selection.names).toEqual([])
  expect(selection.anchor).toBeNull()
})

test('noun names tables, views or both', function () {
  const table: TableRef = { schema: null, name: 't', kind: 'table' }
  const view: TableRef = { schema: null, name: 'v', kind: 'view' }
  expect(TableSelection.noun([table])).toBe('table')
  expect(TableSelection.noun([table, table])).toBe('tables')
  expect(TableSelection.noun([view])).toBe('view')
  expect(TableSelection.noun([view, view])).toBe('views')
  expect(TableSelection.noun([table, view])).toBe('tables and views')
})

test('a drag selects the range from the start row to the row under the mouse', function () {
  const rows = [0, 1, 2, 3, 4]
  const selection = new TableSelection<number>()
  selection.startDrag(2)
  expect(selection.names).toEqual([2])
  expect(selection.dragging).toBe(true)
  selection.dragTo(rows, 4)
  expect(selection.names).toEqual([2, 3, 4])
  selection.dragTo(rows, 3)
  expect(selection.names).toEqual([2, 3])
  selection.dragTo(rows, 0)
  expect(selection.names).toEqual([0, 1, 2])
  selection.endDrag()
  expect(selection.dragging).toBe(false)
  // After the release, moving over rows changes nothing. Shift-click still uses the start row.
  selection.dragTo(rows, 4)
  expect(selection.names).toEqual([0, 1, 2])
  selection.range(rows, 3)
  expect(selection.names).toEqual([2, 3])
})

test('a drag from the empty area starts at the first row the mouse moves over', function () {
  const rows = [0, 1, 2, 3, 4]
  const selection = new TableSelection<number>()
  selection.only(1)
  selection.startEmptyDrag(false)
  expect(selection.names).toEqual([])
  expect(selection.dragging).toBe(true)
  selection.dragTo(rows, 4)
  expect(selection.names).toEqual([4])
  selection.dragTo(rows, 2)
  expect(selection.names).toEqual([2, 3, 4])
  selection.endDrag()
  expect(selection.dragging).toBe(false)
})

test('a Shift drag from the empty area extends from the old anchor', function () {
  const rows = [0, 1, 2, 3, 4]
  const selection = new TableSelection<number>()
  selection.only(1)
  selection.startEmptyDrag(true)
  expect(selection.names).toEqual([1])
  selection.dragTo(rows, 3)
  expect(selection.names).toEqual([1, 2, 3])
})
