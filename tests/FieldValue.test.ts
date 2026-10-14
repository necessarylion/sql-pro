// Checks how the row detail panel shows a value. Run: bun test
import { expect, test } from 'bun:test'
import { DETAIL_LINE_LENGTH } from '../src/constants'
import { FieldValue } from '../src/services/FieldValue'

test('long values and values with a line break are multi-line', function () {
  expect(FieldValue.multiline(null)).toBe(false)
  expect(FieldValue.multiline('')).toBe(false)
  expect(FieldValue.multiline('short')).toBe(false)
  expect(FieldValue.multiline('a'.repeat(DETAIL_LINE_LENGTH))).toBe(false)
  expect(FieldValue.multiline('a'.repeat(DETAIL_LINE_LENGTH + 1))).toBe(true)
  expect(FieldValue.multiline('one\ntwo')).toBe(true)
})

test('a JSON object or array is pretty printed', function () {
  expect(FieldValue.prettyJson('{"a":1,"b":[1,2]}')).toBe(
    '{\n  "a": 1,\n  "b": [\n    1,\n    2\n  ]\n}',
  )
  expect(FieldValue.prettyJson('  [1]  ')).toBe('[\n  1\n]')
  expect(FieldValue.prettyJson('[]')).toBe('[]')
})

test('other values are not JSON', function () {
  expect(FieldValue.prettyJson(null)).toBeNull()
  expect(FieldValue.prettyJson('')).toBeNull()
  expect(FieldValue.prettyJson('42')).toBeNull()
  expect(FieldValue.prettyJson('"text"')).toBeNull()
  expect(FieldValue.prettyJson('true')).toBeNull()
  expect(FieldValue.prettyJson('{broken')).toBeNull()
  expect(FieldValue.prettyJson('[1, 2')).toBeNull()
})
