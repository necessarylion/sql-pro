// A check of the backup logic of the Session, with a fake backend. Run: bun test
import { expect, test } from 'bun:test'
import type { DatabaseApi } from '../src/services/DatabaseApi'
import { Session } from '../src/services/Session'
import { FakePrompt } from './FakePrompt'
import type { ConnectionConfig, DumpProgress, TableRef } from '../src/types'

class FakeApi {
  dumps: { id: string; path: string }[] = []
  /** The progress handler of the running dump. */
  onProgress: ((progress: DumpProgress) => void) | null = null
  /** Ends the running dump. */
  private finish: ((error?: string) => void) | null = null
  cancels: string[] = []

  async dumpDatabase(
    id: string,
    path: string,
    onProgress: (progress: DumpProgress) => void,
  ): Promise<void> {
    if (path === '/nowhere/x.sql') {
      throw 'pg_dump was not found'
    }
    this.dumps.push({ id, path })
    if (path === '/tmp/slow.sql') {
      this.onProgress = onProgress
      await new Promise<void>(this.hold.bind(this))
    }
  }

  private hold(resolve: () => void, reject: (error: string) => void): void {
    this.finish = function end(error?: string): void {
      if (error) {
        reject(error)
      } else {
        resolve()
      }
    }
  }

  end(error?: string): void {
    this.finish?.(error)
  }

  async cancelDump(id: string): Promise<void> {
    this.cancels.push(id)
    this.end('Backup cancelled')
  }

  async cancelQuery(): Promise<void> {
    throw 'not a query'
  }
}

function table(name: string): TableRef {
  return { schema: null, name, kind: 'table' }
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

test('a running backup shows its size, time and table, and can be cancelled', async function () {
  const api = new FakeApi()
  const session = new Session(api as unknown as DatabaseApi, new FakePrompt())
  session.tables = [table('orders'), table('users')]

  const running = session.run(session.backup('/tmp/slow.sql'))
  expect(session.busyText).toBe('Backing up…')
  expect(session.cancellable).toBe(true)

  api.onProgress?.({ bytes: 48 * 1024 * 1024, seconds: 32, table: null, tables: 0 })
  expect(session.busyText).toBe('Backing up… 48 MB, 0:32')
  api.onProgress?.({ bytes: 2048, seconds: 65, table: 'orders', tables: 1 })
  expect(session.busyText).toBe('Backing up… 2 KB, 1:05 · Dumping table orders (1 of 2)')
  // More tables than the list knows: no total.
  api.onProgress?.({ bytes: 3 * 1024 ** 3, seconds: 5, table: 'public.x', tables: 3 })
  expect(session.busyText).toBe('Backing up… 3.0 GB, 0:05 · Dumping table public.x (3)')

  await session.cancel()
  expect(await running).toBe(false)
  expect(api.cancels).toEqual([session.id])
  // A cancel is not an error.
  expect(session.error).toBe('')
  expect(session.status).toBe('Backup cancelled')
  expect(session.busy).toBe(false)
  expect(session.busyText).toBe('Working…')
})

test('a backup that fails while it runs shows the error of the tool', async function () {
  const api = new FakeApi()
  const session = new Session(api as unknown as DatabaseApi, new FakePrompt())
  const running = session.run(session.backup('/tmp/slow.sql'))
  api.end('mysqldump: Got error: 1045')
  expect(await running).toBe(false)
  expect(session.error).toBe('mysqldump: Got error: 1045')
  expect(session.cancellable).toBe(false)
})
