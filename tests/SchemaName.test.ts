import { describe, expect, test } from 'bun:test'
import { SchemaName } from '../src/services/SchemaName'

describe('SchemaName', function () {
  test('hides the default schema of PostgreSQL and SQL Server', function () {
    expect(SchemaName.label('public', 'users')).toBe('users')
    expect(SchemaName.label('dbo', 'Orders')).toBe('Orders')
  })

  test('keeps any other schema', function () {
    expect(SchemaName.label('sales', 'Orders')).toBe('sales.Orders')
  })

  test('shows the bare name when there is no schema', function () {
    expect(SchemaName.label(null, 'users')).toBe('users')
  })
})
