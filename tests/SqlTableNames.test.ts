import { describe, expect, test } from 'bun:test'
import { SqlTableNames } from '../src/services/SqlTableNames'

describe('SqlTableNames', function () {
  test('finds the table after FROM in any case', function () {
    expect(SqlTableNames.find('SELECT\n  *\nfrom\n  admins\nwhere em')).toEqual(['admins'])
  })

  test('finds joined tables with aliases and drops quotes', function () {
    const sql =
      'select * from `users` u join "public"."orders" as o on o.user_id = u.id left join [Items] i'
    expect(SqlTableNames.find(sql)).toEqual(['users', 'public.orders', 'Items'])
  })

  test('finds each table of a comma list', function () {
    expect(SqlTableNames.find('select * from a x, b as y, c where 1')).toEqual(['a', 'b', 'c'])
  })

  test('finds tables of UPDATE and INSERT INTO, each one time', function () {
    expect(SqlTableNames.find('update t set x = 1; insert into t values (1)')).toEqual(['t'])
  })

  test('skips a subquery', function () {
    expect(SqlTableNames.find('select * from (select 1) s')).toEqual([])
  })

  test('resolves an alias to its table', function () {
    const sql = 'select * from admins a join `orders` as o on o.admin_id = a.id where a.em'
    expect(SqlTableNames.resolve(sql, 'a')).toBe('admins')
    expect(SqlTableNames.resolve(sql, 'O')).toBe('orders')
  })

  test('resolves a quoted alias and a quoted table name', function () {
    const sql = 'select * from "public"."admins" "x" where "x".'
    expect(SqlTableNames.resolve(sql, '"x"')).toBe('public.admins')
    expect(SqlTableNames.resolve(sql, '`admins`')).toBe('admins')
  })

  test('does not take a keyword after the table as an alias', function () {
    const sql = 'select * from admins where admins.em'
    expect(SqlTableNames.resolve(sql, 'where')).toBe('where')
    expect(SqlTableNames.find(sql)).toEqual(['admins'])
  })

  test('resolves aliases in a comma list', function () {
    expect(SqlTableNames.resolve('select * from a x, b as y where y.', 'y')).toBe('b')
  })
})
