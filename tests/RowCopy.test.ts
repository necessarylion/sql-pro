// Checks the tab-separated text of copied rows. Run: bun test
import { expect, test } from 'bun:test'
import { RowCopy } from '../src/services/RowCopy'

const copy = new RowCopy()

test('rows become lines and cells are joined by a tab', function () {
  const rows = [
    ['1', 'Ann'],
    ['2', 'Bob'],
  ]
  expect(copy.text(rows, [0, 1])).toBe('1\tAnn\n2\tBob')
})

test('NULL is written as NULL and an empty value stays empty', function () {
  expect(copy.text([['1', null, '']], [0, 1, 2])).toBe('1\tNULL\t')
})

test('tabs, line breaks and quotes are quoted', function () {
  expect(copy.cell('a\tb')).toBe('"a\tb"')
  expect(copy.cell('a\nb')).toBe('"a\nb"')
  expect(copy.cell('a\r\nb')).toBe('"a\r\nb"')
  expect(copy.cell('say "hi"')).toBe('"say ""hi"""')
  expect(copy.cell('plain')).toBe('plain')
})

test('the columns come in the shown order', function () {
  expect(copy.text([['1', 'Ann', 'x']], [2, 0, 1])).toBe('x\t1\tAnn')
})
