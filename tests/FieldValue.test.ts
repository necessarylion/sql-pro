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

test('date and time columns get the right picker on every database', function () {
  // MySQL / MariaDB
  expect(FieldValue.pickerType('date', 'mysql')).toBe('date')
  expect(FieldValue.pickerType('time', 'mysql')).toBe('time')
  expect(FieldValue.pickerType('time(3)', 'mysql')).toBe('time')
  expect(FieldValue.pickerType('datetime', 'mysql')).toBe('datetime-local')
  expect(FieldValue.pickerType('datetime(6)', 'mysql')).toBe('datetime-local')
  expect(FieldValue.pickerType('timestamp', 'mysql')).toBe('datetime-local')
  // PostgreSQL
  expect(FieldValue.pickerType('time without time zone', 'postgres')).toBe('time')
  expect(FieldValue.pickerType('timestamp without time zone', 'postgres')).toBe('datetime-local')
  expect(FieldValue.pickerType('timestamp(3)', 'postgres')).toBe('datetime-local')
  // SQL Server
  expect(FieldValue.pickerType('date', 'mssql')).toBe('date')
  expect(FieldValue.pickerType('time', 'mssql')).toBe('time')
  expect(FieldValue.pickerType('datetime2', 'mssql')).toBe('datetime-local')
  expect(FieldValue.pickerType('smalldatetime', 'mssql')).toBe('datetime-local')
  // SQLite, in any case
  expect(FieldValue.pickerType('DATETIME', 'sqlite')).toBe('datetime-local')
  expect(FieldValue.pickerType('Date', 'sqlite')).toBe('date')
})

test('row versions, time zone types and other types get no picker', function () {
  expect(FieldValue.pickerType('timestamp', 'mssql')).toBeNull()
  expect(FieldValue.pickerType('rowversion', 'mssql')).toBeNull()
  expect(FieldValue.pickerType('timestamptz', 'postgres')).toBeNull()
  expect(FieldValue.pickerType('timestamp with time zone', 'postgres')).toBeNull()
  expect(FieldValue.pickerType('time with time zone', 'postgres')).toBeNull()
  expect(FieldValue.pickerType('timetz', 'postgres')).toBeNull()
  expect(FieldValue.pickerType('datetimeoffset', 'mssql')).toBeNull()
  expect(FieldValue.pickerType('varchar(20)', 'mysql')).toBeNull()
  expect(FieldValue.pickerType('interval', 'postgres')).toBeNull()
  expect(FieldValue.pickerType('', 'sqlite')).toBeNull()
})

test('database text becomes a picker value', function () {
  expect(FieldValue.toPickerValue('2024-01-02', 'date')).toBe('2024-01-02')
  expect(FieldValue.toPickerValue('03:04:05', 'time')).toBe('03:04:05')
  expect(FieldValue.toPickerValue('03:04:05.1234567', 'time')).toBe('03:04:05.123')
  expect(FieldValue.toPickerValue('2024-01-02 03:04:05.123', 'datetime-local')).toBe(
    '2024-01-02T03:04:05.123',
  )
  expect(FieldValue.toPickerValue('2024-01-02T03:04', 'datetime-local')).toBe('2024-01-02T03:04')
})

test('a value the picker cannot read opens it empty', function () {
  expect(FieldValue.toPickerValue(null, 'date')).toBe('')
  expect(FieldValue.toPickerValue('', 'time')).toBe('')
  expect(FieldValue.toPickerValue('yesterday', 'date')).toBe('')
  expect(FieldValue.toPickerValue('838:59:59', 'time')).toBe('')
  expect(FieldValue.toPickerValue('2024-01-02', 'datetime-local')).toBe('')
  expect(FieldValue.toPickerValue('2024-01-02 03:04:05+02', 'datetime-local')).toBe('')
})

test('a picker value becomes database text', function () {
  expect(FieldValue.fromPickerValue('2024-01-02')).toBe('2024-01-02')
  expect(FieldValue.fromPickerValue('03:04:05')).toBe('03:04:05')
  expect(FieldValue.fromPickerValue('03:04')).toBe('03:04:00')
  expect(FieldValue.fromPickerValue('2024-01-02T03:04:05')).toBe('2024-01-02 03:04:05')
  expect(FieldValue.fromPickerValue('2024-01-02T03:04')).toBe('2024-01-02 03:04:00')
})

test('database text splits into the date and time inputs', function () {
  expect(FieldValue.toPickerParts('2024-01-02', 'date')).toEqual({ date: '2024-01-02', time: '' })
  expect(FieldValue.toPickerParts('03:04:05', 'time')).toEqual({ date: '', time: '03:04:05' })
  expect(FieldValue.toPickerParts('2024-01-02 03:04:05', 'datetime-local')).toEqual({
    date: '2024-01-02',
    time: '03:04:05',
  })
  expect(FieldValue.toPickerParts(null, 'datetime-local')).toEqual({ date: '', time: '' })
  expect(FieldValue.toPickerParts('soon', 'date')).toEqual({ date: '', time: '' })
})

test('the date and time inputs join into database text', function () {
  expect(FieldValue.fromPickerParts({ date: '2024-01-02', time: '' }, 'date')).toBe('2024-01-02')
  expect(FieldValue.fromPickerParts({ date: '', time: '' }, 'date')).toBeNull()
  expect(FieldValue.fromPickerParts({ date: '', time: '03:04' }, 'time')).toBe('03:04:00')
  expect(FieldValue.fromPickerParts({ date: '', time: '' }, 'time')).toBeNull()
  expect(
    FieldValue.fromPickerParts({ date: '2024-01-02', time: '03:04:05' }, 'datetime-local'),
  ).toBe('2024-01-02 03:04:05')
  expect(FieldValue.fromPickerParts({ date: '2024-01-02', time: '03:04' }, 'datetime-local')).toBe(
    '2024-01-02 03:04:00',
  )
  // Only a date: the time is midnight.
  expect(FieldValue.fromPickerParts({ date: '2024-01-02', time: '' }, 'datetime-local')).toBe(
    '2024-01-02 00:00:00',
  )
  expect(FieldValue.fromPickerParts({ date: '', time: '03:04:05' }, 'datetime-local')).toBeNull()
})

test('now is the local date and time with two digits', function () {
  expect(FieldValue.nowParts(new Date(2024, 0, 2, 3, 4, 5))).toEqual({
    date: '2024-01-02',
    time: '03:04:05',
  })
})
