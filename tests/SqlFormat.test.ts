// Checks the SQL formatter. Run: bun test
import { expect, test } from 'bun:test'
import { SqlFormat } from '../src/services/SqlFormat'

test('formats SQL on several lines', function () {
  const text = new SqlFormat('mysql').format('select id, name from users where id = 1')
  expect(text).toBe('select\n  id,\n  name\nfrom\n  users\nwhere\n  id = 1')
})

test('uses the dialect of the driver', function () {
  // Backticks quote names only in MySQL. Brackets only in SQL Server.
  expect(new SqlFormat('mysql').format('select `a b` from t')).toContain('`a b`')
  expect(new SqlFormat('mssql').format('select [a b] from t')).toContain('[a b]')
  expect(new SqlFormat('postgres').format('select x::int from t')).toContain('x::int')
  expect(new SqlFormat('sqlite').format('select 1')).toBe('select\n  1')
  expect(new SqlFormat(undefined).format('select 1')).toBe('select\n  1')
})

test('throws on SQL it cannot parse', function () {
  expect(function () {
    new SqlFormat('postgres').format('select (1')
  }).toThrow()
})
