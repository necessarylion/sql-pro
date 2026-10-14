// Checks the export and import of saved connections, with a fake backend. Run: bun test
import { expect, test } from 'bun:test'
import { DEFAULT_CONFIGS, DEFAULT_SSH_CONFIG } from '../src/constants'
import {
  CONNECTIONS_FILE_ERROR,
  CONNECTIONS_FILE_NEWER,
  CONNECTIONS_FILE_VERSION,
} from '../src/constants/transfer'
import { ConnectionTransfer } from '../src/services/ConnectionTransfer'
import type { DatabaseApi } from '../src/services/DatabaseApi'
import { FavoriteStore } from '../src/services/FavoriteStore'
import type { Favorite } from '../src/services/FavoriteStore'
import { ProfileStorage } from '../src/services/ProfileStorage'

class FakeStorage {
  items: Record<string, string> = {}

  getItem(key: string): string | null {
    return this.items[key] ?? null
  }

  setItem(key: string, value: string): void {
    this.items[key] = value
  }
}

class FakeApi {
  files: Record<string, string> = {}

  async readTextFile(path: string): Promise<string> {
    const text = this.files[path]
    if (text === undefined) {
      throw 'No such file'
    }
    return text
  }

  async writeTextFile(path: string, text: string): Promise<void> {
    this.files[path] = text
  }
}

function favorite(id: string, name: string): Favorite {
  const ssh = { ...DEFAULT_SSH_CONFIG, host: 'jump', password: 'tunnel' }
  return {
    id,
    name,
    environment: 'prod',
    folderId: 'f1',
    config: { ...DEFAULT_CONFIGS.mysql, password: 'secret', ssh },
  }
}

function setup(): { api: FakeApi; store: FavoriteStore; transfer: ConnectionTransfer } {
  const api = new FakeApi()
  const storage = new ProfileStorage(new FakeStorage() as unknown as Storage, '')
  const store = new FavoriteStore(api as unknown as DatabaseApi, storage)
  const transfer = new ConnectionTransfer(api as unknown as DatabaseApi, store)
  return { api, store, transfer }
}

test('export writes the version and the connections, with no password', async function () {
  const { api, store, transfer } = setup()
  store.favorites.push(favorite('a', 'Shop'))

  expect(await transfer.exportTo('/out.json')).toBe(1)

  const text = api.files['/out.json']
  expect(text).not.toContain('secret')
  expect(text).not.toContain('tunnel')
  const file = JSON.parse(text)
  expect(file.version).toBe(CONNECTIONS_FILE_VERSION)
  expect(file.connections[0].name).toBe('Shop')
  expect(file.connections[0].config.ssh.host).toBe('jump')
  expect(file.connections[0].folderId).toBeUndefined()
})

test('import adds the connections and gives a taken id a new one', async function () {
  const { api, store, transfer } = setup()
  store.favorites.push(favorite('a', 'Shop'))
  await transfer.exportTo('/out.json')
  api.files['/out.json'] = api.files['/out.json'].replace(
    '"connections": [',
    '"connections": [' + JSON.stringify(favorite('b', 'Blog')) + ',',
  )

  expect(await transfer.importFrom('/out.json')).toBe(2)

  expect(store.favorites.length).toBe(3)
  expect(store.byId('a')?.name).toBe('Shop')
  expect(store.byId('b')?.name).toBe('Blog')
  const copy = store.favorites[2]
  expect(copy.name).toBe('Shop')
  expect(copy.id).not.toBe('a')
  // A password in the file is never taken over.
  expect(store.byId('b')?.config.password).toBe('')
  expect(store.byId('b')?.config.ssh?.password).toBe('')
})

test('a bad file throws a clear error and adds nothing', async function () {
  const { api, store, transfer } = setup()
  const bad: Record<string, string> = {
    '/text': 'not json',
    '/array': '[]',
    '/no-list': '{"version":1}',
    '/bad-driver': JSON.stringify({
      version: 1,
      connections: [
        { ...favorite('a', 'x'), config: { ...DEFAULT_CONFIGS.mysql, driver: 'oracle' } },
      ],
    }),
    '/bad-ssh': JSON.stringify({
      version: 1,
      connections: [
        { ...favorite('a', 'x'), config: { ...DEFAULT_CONFIGS.mysql, ssh: { host: 1 } } },
      ],
    }),
  }
  Object.assign(api.files, bad)
  for (const path of Object.keys(bad)) {
    await expect(transfer.importFrom(path)).rejects.toThrow(CONNECTIONS_FILE_ERROR)
  }
  api.files['/new'] = JSON.stringify({ version: CONNECTIONS_FILE_VERSION + 1, connections: [] })
  await expect(transfer.importFrom('/new')).rejects.toThrow(CONNECTIONS_FILE_NEWER)
  expect(store.favorites.length).toBe(0)
})

test('export of one connection writes only that one, with no password', async function () {
  const { api, store, transfer } = setup()
  store.favorites.push(favorite('a', 'Shop'), favorite('b', 'Blog'))

  expect(await transfer.exportTo('/one.json', [store.favorites[1]])).toBe(1)

  const text = api.files['/one.json']
  expect(text).not.toContain('secret')
  expect(text).not.toContain('tunnel')
  const connections = ConnectionTransfer.parse(text)
  expect(connections.length).toBe(1)
  expect(connections[0].name).toBe('Blog')
  expect(ConnectionTransfer.fileName(' Shop DB / prod ')).toBe('Shop-DB-prod.json')
  expect(ConnectionTransfer.fileName('///')).toBe('sql-pro-connections.json')
})
