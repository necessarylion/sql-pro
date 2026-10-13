// Checks which SQL the Query view asks about before it runs. Run: bun test
import { expect, test } from 'bun:test'
import { SqlRisk } from '../src/services/SqlRisk'

const risk = new SqlRisk('mysql')
const postgres = new SqlRisk('postgres')

test('safe statements give no warning', function () {
  const safe = [
    '',
    '  ;; ',
    'SELECT * FROM users',
    "INSERT INTO users (name) VALUES ('Ann')",
    'UPDATE users SET name = 1 WHERE id = 2',
    'delete from users\nwhere id = 2;',
    'SHOW TABLES',
    'EXPLAIN SELECT 1',
    'CREATE TABLE dropped (id int)',
  ]
  for (const sql of safe) {
    expect(risk.find(sql)).toEqual([])
    expect(risk.message(sql)).toBe('')
  }
})

test('each kind of risky statement is found, in any letter case', function () {
  expect(risk.find('DROP TABLE orders')).toEqual(['DROP: DROP TABLE orders'])
  expect(risk.find('drop database shop;')).toEqual(['DROP: drop database shop'])
  expect(risk.find('Truncate  orders')).toEqual(['TRUNCATE: Truncate orders'])
  expect(risk.find('alter table t\n  drop column a')).toEqual([
    'ALTER: alter table t drop column a',
  ])
  expect(risk.find('RENAME TABLE a TO b')).toEqual(['RENAME: RENAME TABLE a TO b'])
  expect(risk.find('DELETE FROM users')).toEqual(['DELETE without WHERE: DELETE FROM users'])
  expect(risk.find('uPdAtE t SET a = 1')).toEqual(['UPDATE without WHERE: uPdAtE t SET a = 1'])
})

test('strings and quoted names are not read as SQL', function () {
  expect(risk.find("SELECT 'drop table x; delete from y'")).toEqual([])
  expect(risk.find("SELECT 'it''s; DROP TABLE x'")).toEqual([])
  expect(risk.find("SELECT 'a\\'; DROP TABLE x'")).toEqual([])
  expect(risk.find('SELECT `a;b`, "c;d" FROM t')).toEqual([])
  expect(risk.find("UPDATE t SET note = 'no where here'")).toEqual([
    "UPDATE without WHERE: UPDATE t SET note = 'no where here'",
  ])
  expect(risk.find('DELETE FROM `where`')).toEqual(['DELETE without WHERE: DELETE FROM `where`'])
  expect(risk.find('UPDATE t SET no_where = 1')).toHaveLength(1)
})

test('comments are not read as SQL', function () {
  expect(risk.find('-- DROP TABLE x\nSELECT 1')).toEqual([])
  expect(risk.find('# DROP TABLE x\nSELECT 1')).toEqual([])
  expect(risk.find('SELECT 1 /* ; DROP TABLE x */')).toEqual([])
  expect(risk.find('/* clean up */ -- now\n  DROP TABLE x')).toEqual(['DROP: DROP TABLE x'])
  expect(risk.find('DELETE FROM t -- where id = 1')).toHaveLength(1)
  expect(risk.find('DELETE FROM t /* where */')).toHaveLength(1)
})

test('the driver decides what a comment and an escape is', function () {
  // In PostgreSQL a backslash does not escape and # is an operator.
  expect(postgres.find("SELECT 'a\\'; DROP TABLE x")).toEqual(['DROP: DROP TABLE x'])
  expect(postgres.find('SELECT 1 # 2; DROP TABLE x')).toEqual(['DROP: DROP TABLE x'])
  // In MySQL, -- needs a space after it, and the code in /*! */ runs.
  expect(risk.find('SELECT 1--1; DROP TABLE x')).toEqual(['DROP: DROP TABLE x'])
  expect(risk.find('/*!40000 ALTER TABLE t DISABLE KEYS */')).toHaveLength(1)
})

test('several statements: only the risky ones are listed', function () {
  const sql = "SELECT 1; DROP TABLE a;\nINSERT INTO b VALUES (';'); delete from c; SELECT 2"
  expect(risk.find(sql)).toEqual(['DROP: DROP TABLE a', 'DELETE without WHERE: delete from c'])
  expect(risk.message(sql)).toStartWith('DROP: DROP TABLE a\nDELETE without WHERE: delete from c\n')
})

test('a long statement is cut and a long list ends with "and N more"', function () {
  const long = risk.find(`DROP TABLE ${'x'.repeat(200)}`)[0]
  expect(long).toEndWith('…')
  expect(long.length).toBeLessThan(80)

  const lines = risk.message('DROP TABLE t;'.repeat(8)).split('\n')
  expect(lines.slice(0, 6)).toEqual([
    'DROP: DROP TABLE t',
    'DROP: DROP TABLE t',
    'DROP: DROP TABLE t',
    'DROP: DROP TABLE t',
    'DROP: DROP TABLE t',
    'and 3 more',
  ])
})
