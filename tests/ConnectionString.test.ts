// Checks the reading of a pasted connection string. Run: bun test
import { expect, test } from 'bun:test'
import { CONNECTION_STRING_ERROR, DEFAULT_CONFIGS } from '../src/constants'
import { ConnectionString } from '../src/services/ConnectionString'

/** The error text of a bad connection string. '' when it was read fine. */
function errorOf(text: string): string {
  try {
    ConnectionString.parse(text)
    return ''
  } catch (error) {
    return (error as Error).message
  }
}

test('reads a mysql string', function () {
  expect(ConnectionString.parse('mysql://root:secret@127.0.0.1:3307/app')).toEqual({
    driver: 'mysql',
    host: '127.0.0.1',
    port: 3307,
    user: 'root',
    password: 'secret',
    database: 'app',
    ssh: null,
  })
})

test('reads a postgres string and ignores the options', function () {
  const text = '  postgres://user:secret@db.example.com:5433/shop?sslmode=require#x  '
  expect(ConnectionString.parse(text)).toEqual({
    driver: 'postgres',
    host: 'db.example.com',
    port: 5433,
    user: 'user',
    password: 'secret',
    database: 'shop',
    ssh: null,
  })
})

test('mariadb is mysql and postgresql is postgres', function () {
  expect(ConnectionString.parse('mariadb://u:p@h/db').driver).toBe('mysql')
  expect(ConnectionString.parse('postgresql://u:p@h/db').driver).toBe('postgres')
  expect(ConnectionString.parse('POSTGRES://u:p@h/db').driver).toBe('postgres')
})

test('mssql and sqlserver are SQL Server, with its defaults', function () {
  expect(ConnectionString.parse('sqlserver://u:p@h/db').driver).toBe('mssql')
  expect(ConnectionString.parse('mssql://db.example.com')).toEqual({
    ...DEFAULT_CONFIGS.mssql,
    host: 'db.example.com',
  })
})

test('missing parts keep the defaults', function () {
  expect(ConnectionString.parse('mysql://db.example.com')).toEqual({
    ...DEFAULT_CONFIGS.mysql,
    host: 'db.example.com',
  })
  expect(ConnectionString.parse('postgres://db.example.com')).toEqual({
    ...DEFAULT_CONFIGS.postgres,
    host: 'db.example.com',
  })
  expect(ConnectionString.parse('mysql://u:p@h/').database).toBe('')
})

test('user, password and database are percent-decoded', function () {
  const config = ConnectionString.parse('postgres://my%40user:p%40ss%2Fw%3Ard@h/my%20db')
  expect(config.user).toBe('my@user')
  expect(config.password).toBe('p@ss/w:rd')
  expect(config.database).toBe('my db')
})

test('an IPv6 host loses its brackets', function () {
  const config = ConnectionString.parse('mysql://u:p@[::1]:3307/db')
  expect(config.host).toBe('::1')
  expect(config.port).toBe(3307)
})

test('reads every sqlite form', function () {
  for (const scheme of ['sqlite', 'file']) {
    expect(ConnectionString.parse(`${scheme}:///abs/path.db`).database).toBe('/abs/path.db')
    expect(ConnectionString.parse(`${scheme}:/abs/path.db`).database).toBe('/abs/path.db')
    expect(ConnectionString.parse(`${scheme}:relative.db`).database).toBe('relative.db')
    expect(ConnectionString.parse(`${scheme}://relative.db`).database).toBe('relative.db')
    expect(ConnectionString.parse(`${scheme}://Dir/sub/a.db`).database).toBe('Dir/sub/a.db')
    expect(ConnectionString.parse(`${scheme}:///C:/data/app.db`).database).toBe('C:/data/app.db')
    expect(ConnectionString.parse(`${scheme}:///abs/my%20db.db?mode=ro`).database).toBe(
      '/abs/my db.db',
    )
  }
  expect(ConnectionString.parse('sqlite:///abs/path.db')).toEqual({
    ...DEFAULT_CONFIGS.sqlite,
    database: '/abs/path.db',
  })
})

test('bad text gives the plain error', function () {
  const bad = ['', '   ', 'garbage', 'http://example.com/db', 'redis://h:6379', 'sqlite:']
  for (const text of bad) {
    expect(errorOf(text)).toBe(CONNECTION_STRING_ERROR)
  }
})

test('the error never holds the password', function () {
  // A bad percent code, a bad port and an unknown scheme.
  const bad = ['postgres://u:hunter2%ZZ@h/db', 'mysql://u:hunter2@h:99999/db', 'x://u:hunter2@h/db']
  for (const text of bad) {
    expect(errorOf(text)).toBe(CONNECTION_STRING_ERROR)
    expect(errorOf(text)).not.toContain('hunter2')
  }
})
