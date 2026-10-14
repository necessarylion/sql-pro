// Checks what a drop means for each row under the mouse. Run: bun test
import { expect, test } from 'bun:test'
import { DropPlan } from '../src/services/DropPlan'
import type { DropRow } from '../src/services/DropPlan'

// Work (a, b), Home (closed, so no rows), then c and d at the top level.
const ROWS: DropRow[] = [
  { folderId: 'work', favoriteId: '' },
  { folderId: 'work', favoriteId: 'a' },
  { folderId: 'work', favoriteId: 'b' },
  { folderId: 'home', favoriteId: '' },
  { folderId: '', favoriteId: 'c' },
  { folderId: '', favoriteId: 'd' },
]

test('a connection over a connection row goes before or after it', function () {
  expect(DropPlan.decide(ROWS, false, 2, true)).toEqual({
    folderId: 'work',
    anchorId: 'b',
    after: false,
    into: null,
    line: 2,
    lineAfter: false,
  })
  const lower = DropPlan.decide(ROWS, false, 4, false)
  expect(lower.folderId).toBe('')
  expect(lower.anchorId).toBe('c')
  expect(lower.after).toBe(true)
  expect(lower.line).toBe(4)
  expect(lower.lineAfter).toBe(true)
})

test('a connection over a folder row goes into the folder, at the end', function () {
  for (const upper of [true, false]) {
    const drop = DropPlan.decide(ROWS, false, 3, upper)
    expect(drop.folderId).toBe('home')
    expect(drop.anchorId).toBe('')
    expect(drop.into).toBe('home')
    expect(drop.line).toBe(-1)
  }
})

test('a connection over the empty part goes to the end of the top level', function () {
  const drop = DropPlan.decide(ROWS, false, -1, false)
  expect(drop.folderId).toBe('')
  expect(drop.anchorId).toBe('')
  expect(drop.into).toBe('')
  expect(drop.line).toBe(-1)
})

test('a folder over a folder row goes before or after it', function () {
  const before = DropPlan.decide(ROWS, true, 3, true)
  expect(before.anchorId).toBe('home')
  expect(before.after).toBe(false)
  expect(before.into).toBeNull()
  expect(before.line).toBe(3)
  expect(before.lineAfter).toBe(false)

  // After an open folder: the line is under its last connection.
  const after = DropPlan.decide(ROWS, true, 0, false)
  expect(after.anchorId).toBe('work')
  expect(after.after).toBe(true)
  expect(after.line).toBe(2)
  expect(after.lineAfter).toBe(true)
})

test('a folder over a connection row counts as the folder of that row', function () {
  for (const upper of [true, false]) {
    const drop = DropPlan.decide(ROWS, true, 1, upper)
    expect(drop.anchorId).toBe('work')
    expect(drop.after).toBe(true)
    expect(drop.into).toBeNull()
    expect(drop.line).toBe(2)
  }
})

test('a folder over the top level or the empty part goes after the last folder', function () {
  for (const index of [4, -1]) {
    const drop = DropPlan.decide(ROWS, true, index, true)
    expect(drop.anchorId).toBe('home')
    expect(drop.after).toBe(true)
    expect(drop.line).toBe(3)
    expect(drop.lineAfter).toBe(true)
  }
})

test('next gives the id after an id, or nothing', function () {
  const list = [{ id: 'a' }, { id: 'b' }]
  expect(DropPlan.next(list, 'a')).toBe('b')
  expect(DropPlan.next(list, 'b')).toBe('')
  expect(DropPlan.next(list, 'missing')).toBe('')
})
