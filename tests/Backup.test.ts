// A check of the backup logic of the Session, with a fake backend. Run: bun test
import { expect, test } from 'bun:test'
import type { DatabaseApi } from '../src/services/DatabaseApi'
import { Session } from '../src/services/Session'
import { FakePrompt } from './FakePrompt'
import { PgToolChoice } from '../src/services/PgToolChoice'
import type {
  BackupOptions,
  ConnectionConfig,
  DumpProgress,
  RestoreOptions,
  TableRef,
} from '../src/types'

class FakeApi {
  dumps: { id: string; path: string }[] = []
  /** The progress handler of the running dump. */
  onProgress: ((progress: DumpProgress) => void) | null = null
  /** Ends the running dump. */
  private finish: ((error?: string) => void) | null = null
  cancels: string[] = []

  options: BackupOptions[] = []
  restores: { path: string; options: RestoreOptions }[] = []
  tableLoads = 0

  async dumpDatabase(
    id: string,
    path: string,
    options: BackupOptions,
    onProgress: (progress: DumpProgress) => void,
  ): Promise<void> {
    this.options.push(options)
    if (path === '/nowhere/x.sql') {
      throw 'pg_dump was not found'
    }
    this.dumps.push({ id, path })
    if (path === '/tmp/slow.sql') {
      this.onProgress = onProgress
      await new Promise<void>(this.hold.bind(this))
    }
  }

  async restoreDump(
    id: string,
    path: string,
    options: RestoreOptions,
    onProgress: (progress: DumpProgress) => void,
  ): Promise<void> {
    this.restores.push({ path, options })
    this.onProgress = onProgress
    await new Promise<void>(this.hold.bind(this))
  }

  /** When set, loading the tables fails with this error. */
  tablesError = ''

  async listTables(): Promise<TableRef[]> {
    this.tableLoads += 1
    if (this.tablesError) {
      throw this.tablesError
    }
    return [table('restored')]
  }

  async listObjects(): Promise<never[]> {
    return []
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

  queryCancels: string[] = []
  /** The page load that runs now. It waits until it is cancelled or ended by the test. */
  private page: PromiseWithResolvers<never> | null = null

  async cancelQuery(id: string): Promise<void> {
    this.queryCancels.push(id)
    this.page?.reject('Query cancelled')
  }

  browse(): Promise<never> {
    this.page = Promise.withResolvers<never>()
    return this.page.promise
  }

  /** Ends the running page load with a normal error. */
  failPage(error: string): void {
    this.page?.reject(error)
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
  expect(session.dumping).toBe(true)
  expect(session.canCancel).toBe(true)

  api.onProgress?.({ bytes: 48 * 1024 * 1024, seconds: 32, table: null, tables: 0 })
  expect(session.busyText).toBe('Backing up… 48 MB, 0:32')
  api.onProgress?.({ bytes: 2048, seconds: 65, table: 'orders', tables: 1 })
  expect(session.busyText).toBe('Backing up… 2 KB, 1:05 · Dumping table orders (1 of 2)')
  // More tables than the list knows: no total.
  api.onProgress?.({ bytes: 3 * 1024 ** 3, seconds: 5, table: 'public.x', tables: 3 })
  expect(session.busyText).toBe('Backing up… 3.0 GB, 0:05 · Dumping table public.x (3)')

  await session.cancel()
  expect(await running).toBe(true)
  expect(api.cancels).toEqual([session.id])
  // A cancel is not an error.
  expect(session.error).toBe('')
  expect(session.status).toBe('Backup cancelled')
  expect(session.busy).toBe(false)
  expect(session.busyText).toBe('Working…')
  expect(session.canCancel).toBe(false)
})

test('a cancelled page load during a backup keeps the backup cancellable', async function () {
  const api = new FakeApi()
  const session = new Session(api as unknown as DatabaseApi, new FakePrompt())
  const running = session.run(session.backup('/tmp/slow.sql'))

  // The user opens a table. Esc stops that page load, not the backup.
  const load = session.run(session.selectTable(table('orders')))
  // selectTable asks about unsaved changes first. Let it reach the page load.
  await Bun.sleep(0)
  expect(session.cancelTitle).toBe('Stop the running query (Esc)')
  await session.cancel()
  expect(await load).toBe(false)
  expect(api.queryCancels).toEqual([session.id])
  expect(api.cancels).toEqual([])
  expect(session.status).toBe('Query cancelled')
  expect(session.error).toBe('')

  // The backup still runs and can still be cancelled.
  expect(session.canCancel).toBe(true)
  expect(session.cancelPending).toBe(false)
  expect(session.cancelTitle).toBe('Stop the backup (Esc)')

  // A second backup in the same tab is refused. The first one goes on.
  expect(await session.run(session.backup('/tmp/other.sql'))).toBe(false)
  expect(session.error).toBe('A backup already runs in this tab')
  expect(session.dumping).toBe(true)
  expect(session.backupProgress).toBe('Backing up…')

  // The cancel of the page load does not make the backup look cancelled.
  api.end()
  expect(await running).toBe(true)
  expect(session.status).toBe('Backup saved to /tmp/slow.sql')
  expect(session.busy).toBe(false)
})

test('a page load that fails during a backup leaves Cancel to the backup', async function () {
  const api = new FakeApi()
  const session = new Session(api as unknown as DatabaseApi, new FakePrompt())
  const running = session.run(session.backup('/tmp/slow.sql'))

  const load = session.run(session.selectTable(table('orders')))
  // selectTable asks about unsaved changes first. Let it reach the page load.
  await Bun.sleep(0)
  api.failPage('no such table: orders')
  expect(await load).toBe(false)
  expect(session.error).toBe('no such table: orders')
  expect(session.cancellable).toBe(false)

  // Cancel now stops the backup.
  expect(session.cancelTitle).toBe('Stop the backup (Esc)')
  await session.cancel()
  expect(api.queryCancels).toEqual([])
  expect(api.cancels).toEqual([session.id])
  expect(await running).toBe(true)
  expect(session.status).toBe('Backup cancelled')
  expect(session.dumping).toBe(false)
})

test('a backup that fails while it runs shows the error of the tool', async function () {
  const api = new FakeApi()
  const session = new Session(api as unknown as DatabaseApi, new FakePrompt())
  const running = session.run(session.backup('/tmp/slow.sql'))
  api.end('mysqldump: Got error: 1045')
  expect(await running).toBe(false)
  expect(session.error).toBe('mysqldump: Got error: 1045')
  expect(session.dumping).toBe(false)
})

test('a progress message that arrives after the backup ended is ignored', async function () {
  const api = new FakeApi()
  const session = new Session(api as unknown as DatabaseApi, new FakePrompt())
  const running = session.run(session.backup('/tmp/slow.sql'))
  api.end()
  expect(await running).toBe(true)

  // Tauri can send the last progress after the reply of the command.
  api.onProgress?.({ bytes: 4096, seconds: 3, table: 'orders', tables: 1 })
  expect(session.backupProgress).toBe('')
  expect(session.busyText).toBe('Working…')
})

test('a custom backup sends its options and gets a .dump name', async function () {
  const api = new FakeApi()
  const session = new Session(api as unknown as DatabaseApi, new FakePrompt())
  session.config = config('postgres', 'shop')
  expect(session.backupName(new Date(2026, 0, 31, 9, 5), 'dump')).toBe('shop-2026-01-31-0905.dump')
  await session.run(session.backup('/tmp/shop.dump', { custom: true, version: 17 }))
  // The plain SQL backup is the default.
  await session.run(session.backup('/tmp/shop.sql'))
  expect(api.options).toEqual([
    { custom: true, version: 17 },
    { custom: false, version: null },
  ])
})

test('a restore shows its time and table, then loads the tables again', async function () {
  const api = new FakeApi()
  const session = new Session(api as unknown as DatabaseApi, new FakePrompt())
  const running = session.run(session.restore('/tmp/shop.dump', { version: 17, clean: true }))
  expect(api.restores).toEqual([{ path: '/tmp/shop.dump', options: { version: 17, clean: true } }])
  expect(session.busyText).toBe('Restoring…')
  expect(session.cancelTitle).toBe('Stop the restore (Esc)')
  api.onProgress?.({ bytes: 4096, seconds: 75, table: 'public.orders', tables: 2 })
  expect(session.busyText).toBe('Restoring… 1:15 · Loading table public.orders (2)')
  // A backup cannot start while it runs.
  expect(await session.run(session.backup('/tmp/x.sql'))).toBe(false)
  expect(session.error).toBe('A restore already runs in this tab')
  api.end()
  expect(await running).toBe(true)
  expect(session.status).toBe('Restored /tmp/shop.dump')
  expect(session.tables).toEqual([table('restored')])
  expect(session.dumping).toBe(false)
})

test('a failed or cancelled restore still loads the tables again', async function () {
  const api = new FakeApi()
  const session = new Session(api as unknown as DatabaseApi, new FakePrompt())
  const failing = session.run(session.restore('/tmp/a.dump', { version: null, clean: false }))
  api.end('pg_restore had 2 errors. The rest was restored.')
  expect(await failing).toBe(false)
  expect(session.error).toBe('pg_restore had 2 errors. The rest was restored.')
  expect(api.tableLoads).toBe(1)

  const cancelled = session.run(session.restore('/tmp/a.dump', { version: null, clean: false }))
  await session.cancel()
  expect(await cancelled).toBe(true)
  expect(session.error).toBe('')
  expect(session.status).toBe('Restore cancelled. What was loaded so far stays.')
  expect(api.tableLoads).toBe(2)
})

test('the tool choice warns about a tool older than the server', function () {
  const tools = [
    { version: 18, path: '/a/pg_dump' },
    { version: 15, path: '/b/pg_dump' },
  ]
  const choice = new PgToolChoice('pg_dump', { server: 17, tools, defaultVersion: 18 })
  expect(choice.version).toBe(18)
  expect(choice.label(tools[1]!)).toBe('PostgreSQL 15')
  expect(choice.warning).toBe('')
  expect(choice.missing).toBe('')
  choice.version = 15
  expect(choice.warning).toBe(
    'pg_dump 15 is older than the server (PostgreSQL 17). pg_dump does not back up a newer server.',
  )
  const none = new PgToolChoice('pg_restore', { server: 17, tools: [], defaultVersion: null })
  expect(none.warning).toBe('')
  expect(none.missing).toContain('pg_restore was not found')
})

test('a failed restore keeps its error when the reload of the tables fails too', async function () {
  const api = new FakeApi()
  const session = new Session(api as unknown as DatabaseApi, new FakePrompt())
  api.tablesError = 'connection lost'
  const failing = session.run(session.restore('/tmp/a.dump', { version: null, clean: false }))
  api.end('pg_restore had 1 error. The rest was restored.')
  expect(await failing).toBe(false)
  expect(session.error).toBe('pg_restore had 1 error. The rest was restored.')
  expect(api.tableLoads).toBe(1)
})
