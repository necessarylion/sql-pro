// A check of the backup logic of the Session, with a fake backend. Run: bun test
import { expect, test } from 'bun:test'
import type { DatabaseApi } from '../src/services/DatabaseApi'
import { Session } from '../src/services/Session'
import { FakePrompt } from './FakePrompt'
import type { ConnectionConfig } from '../src/types'

class FakeApi {
  dumps: { id: string; path: string }[] = []

  async dumpDatabase(id: string, path: string): Promise<void> {
    if (path === '/nowhere/x.sql') {
      throw 'pg_dump was not found'
    }
    this.dumps.push({ id, path })
  }
}

function config(driver: ConnectionConfig['driver'], database: string): ConnectionConfig {
  return { driver, host: 'db', port: 1, user: 'me', password: '', database, ssh: null }
}

test('backup sends the tab id and the path, then reports where the file is', async function () {
  const api = new FakeApi()
  const session = new Session(api as unknown as DatabaseApi, new FakePrompt())

  expect(await session.run(session.backup('/tmp/shop.sql'))).toBe(true)
  expect(api.dumps).toEqual([{ id: session.id, path: '/tmp/shop.sql' }])
  expect(session.status).toBe('Backup saved to /tmp/shop.sql')

  // A failure shows the error of the tool and keeps the tab usable.
  expect(await session.run(session.backup('/nowhere/x.sql'))).toBe(false)
  expect(session.error).toBe('pg_dump was not found')
  expect(session.busy).toBe(false)
})

test('the backup file name has the database name and the local time', function () {
  const session = new Session(new FakeApi() as unknown as DatabaseApi, new FakePrompt())
  const now = new Date(2026, 0, 31, 9, 5)
  expect(session.backupName(now)).toBe('backup-2026-01-31-0905.sql')

  session.config = config('mysql', 'shop')
  expect(session.backupName(now)).toBe('shop-2026-01-31-0905.sql')

  // SQLite: the file name without its folder and its ending.
  session.config = config('sqlite', '/Users/me/data/notes.v2.sqlite')
  expect(session.backupName(now)).toBe('notes.v2-2026-01-31-0905.sql')
})
