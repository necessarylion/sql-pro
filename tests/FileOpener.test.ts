// Checks that SQLite files opened from the OS get a connected tab each. Run: bun test
import { expect, test } from 'bun:test'
import type { DatabaseApi } from '../src/services/DatabaseApi'
import { FileOpener } from '../src/services/FileOpener'
import { SessionManager } from '../src/services/SessionManager'
import type { ConnectionConfig, TableRef, UserList } from '../src/types'
import { FakePrompt } from './FakePrompt'

const USERS: TableRef = { schema: null, name: 'users', kind: 'table' }

class FakeApi {
  waiting: string[] = []
  connects: { id: string; config: ConnectionConfig }[] = []
  /** The event handler. It gives back the promise of its work, so a test can wait for it. */
  handler: (() => unknown) | null = null

  async takeOpenedFiles(): Promise<string[]> {
    const paths = this.waiting
    this.waiting = []
    return paths
  }

  async onOpenedFiles(handler: () => unknown): Promise<() => void> {
    this.handler = handler
    return this.stop.bind(this)
  }

  stop(): void {
    this.handler = null
  }

  async connect(id: string, config: ConnectionConfig): Promise<void> {
    this.connects.push({ id, config })
  }

  async listDatabases(): Promise<string[]> {
    return ['main']
  }

  async listTables(): Promise<TableRef[]> {
    return [USERS]
  }

  async listUsers(): Promise<UserList> {
    return { canManage: false, users: [] }
  }
}

class FakeLayout {
  closeDetail(): void {}
}

function setUp(): { api: FakeApi; manager: SessionManager; opener: FileOpener } {
  const api = new FakeApi()
  const manager = new SessionManager(
    api as unknown as DatabaseApi,
    new FakePrompt(),
    new FakeLayout(),
  )
  return { api, manager, opener: new FileOpener(api, manager) }
}

test('files from before the start open in the start tab and new tabs', async function () {
  const { api, manager, opener } = setUp()
  const start = manager.sessions[0]
  api.waiting = ['/data/shop.db', 'C:\\data\\logs.sqlite']
  await opener.start()

  expect(manager.sessions.length).toBe(2)
  expect(manager.sessions[0]).toBe(start)
  expect(start.title).toBe('shop.db')
  expect(start.config?.driver).toBe('sqlite')
  expect(start.config?.database).toBe('/data/shop.db')
  expect(start.tables).toEqual([USERS])
  const second = manager.sessions[1]
  expect(second.title).toBe('logs.sqlite')
  expect(manager.activeId).toBe(second.id)
  expect(api.connects.length).toBe(2)
})

test('a file that comes later opens in a new tab, once', async function () {
  const { api, manager, opener } = setUp()
  await opener.start()
  await manager.openFile('/data/first.db')

  api.waiting = ['/data/later.db']
  await api.handler?.()
  // The event comes again, but the file was already taken.
  await api.handler?.()

  expect(manager.sessions.length).toBe(2)
  expect(manager.sessions[1].config?.database).toBe('/data/later.db')
  expect(api.connects.length).toBe(2)
})
