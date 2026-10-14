// Checks what stays when the database type of a connection changes. Run: bun test
import { expect, test } from 'bun:test'
import { DEFAULT_CONFIGS, DEFAULT_SSH_CONFIG } from '../src/constants'
import { DriverChange } from '../src/services/DriverChange'
import type { ConnectionConfig } from '../src/types'

/** A filled-in MySQL connection with the default port and user. */
function mysql(): ConnectionConfig {
  return {
    ...DEFAULT_CONFIGS.mysql,
    host: 'db.example.com',
    password: 'secret',
    database: 'shop',
    ssh: { ...DEFAULT_SSH_CONFIG, host: 'jump.example.com', user: 'deploy', password: 'sshpw' },
  }
}

test('mysql to postgres keeps host, password, database and ssh', function () {
  const config = mysql()
  const changed = DriverChange.apply(config, 'postgres')
  expect(changed.driver).toBe('postgres')
  expect(changed.host).toBe('db.example.com')
  expect(changed.password).toBe('secret')
  expect(changed.database).toBe('shop')
  expect(changed.ssh).toEqual(config.ssh)
})

test('the default port and user switch to the new defaults', function () {
  const toPostgres = DriverChange.apply(mysql(), 'postgres')
  expect(toPostgres.port).toBe(DEFAULT_CONFIGS.postgres.port)
  expect(toPostgres.user).toBe(DEFAULT_CONFIGS.postgres.user)
  const back = DriverChange.apply(toPostgres, 'mysql')
  expect(back).toEqual(mysql())
})

test('a custom port and user stay', function () {
  const changed = DriverChange.apply({ ...mysql(), port: 3307, user: 'app' }, 'postgres')
  expect(changed.port).toBe(3307)
  expect(changed.user).toBe('app')
})

test('a port typed as text still counts as the default', function () {
  const typed = { ...mysql(), port: '3306' as unknown as number }
  expect(DriverChange.apply(typed, 'postgres').port).toBe(DEFAULT_CONFIGS.postgres.port)
})

test('to sqlite and from sqlite give the defaults', function () {
  expect(DriverChange.apply(mysql(), 'sqlite')).toEqual(DEFAULT_CONFIGS.sqlite)
  const file = { ...DEFAULT_CONFIGS.sqlite, database: '/data/app.db' }
  expect(DriverChange.apply(file, 'postgres')).toEqual(DEFAULT_CONFIGS.postgres)
  expect(DriverChange.apply(file, 'mysql')).toEqual(DEFAULT_CONFIGS.mysql)
})

test('the defaults are copied, not shared', function () {
  expect(DriverChange.apply(mysql(), 'sqlite')).not.toBe(DEFAULT_CONFIGS.sqlite)
})

test('the same driver gives the same values', function () {
  const config = { ...mysql(), port: 3307, user: 'app' }
  expect(DriverChange.apply(config, 'mysql')).toEqual(config)
})
