// Checks the folders of the saved connections. Run: bun test
import { afterEach, beforeEach, expect, test } from 'bun:test'
import { DEFAULT_CONFIGS, FAVORITES_STORAGE_KEY } from '../src/constants'
import { FOLDERS_STORAGE_KEY } from '../src/constants/folders'
import type { DatabaseApi } from '../src/services/DatabaseApi'
import { FavoriteStore } from '../src/services/FavoriteStore'
import type { FavoriteSection } from '../src/services/FavoriteStore'
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
  secrets: Record<string, string> = {}

  async savePassword(id: string, password: string): Promise<void> {
    this.secrets[id] = password
  }

  async loadPassword(id: string): Promise<string> {
    return this.secrets[id] ?? ''
  }

  async deletePassword(id: string): Promise<void> {
    delete this.secrets[id]
  }
}

let raw = new FakeStorage()
let api = new FakeApi()

beforeEach(useFreshStorage)
afterEach(forgetProfile)

function useFreshStorage(): void {
  raw = new FakeStorage()
  api = new FakeApi()
}

function forgetProfile(): void {
  delete globalThis.__SQL_PRO_PROFILE__
}

/** A new store on the same storage. It reads the saved data again. */
function newStore(): FavoriteStore {
  const storage = new ProfileStorage(raw as unknown as Storage, ProfileStorage.current())
  return new FavoriteStore(api as unknown as DatabaseApi, storage)
}

async function add(store: FavoriteStore, id: string, name: string, folderId = ''): Promise<void> {
  const config = { ...DEFAULT_CONFIGS.mysql, password: 'secret' }
  await store.save({ id, name, environment: 'local', folderId, config })
}

/** The id of the folder with this name. */
function idOf(store: FavoriteStore, name: string): string {
  for (const folder of store.folders) {
    if (folder.name === name) {
      return folder.id
    }
  }
  throw new Error(`No folder ${name}`)
}

/** A short text for a list, like `Work[Shop,Lab] | [Home]`. A closed folder has `(closed)`. */
function shape(sections: FavoriteSection[]): string {
  const parts: string[] = []
  for (const section of sections) {
    const names: string[] = []
    for (const favorite of section.favorites) {
      names.push(favorite.name)
    }
    const closed = section.open ? '' : '(closed)'
    parts.push(`${section.folder?.name ?? ''}${closed}[${names.join(',')}]`)
  }
  return parts.join(' | ')
}

test('create, rename and recolour a folder', function () {
  const store = newStore()
  expect(store.saveFolder('', '  Work  ', 'red', 'server')).toBe(true)
  const id = idOf(store, 'Work')
  expect(store.folders[0]).toEqual({
    id,
    name: 'Work',
    color: 'red',
    icon: 'server',
    collapsed: false,
  })

  expect(store.saveFolder(id, 'Office', 'blue', 'cloud')).toBe(true)
  expect(store.folders.length).toBe(1)
  // A new store reads the folder back from the storage.
  expect(newStore().folders[0]).toEqual({
    id,
    name: 'Office',
    color: 'blue',
    icon: 'cloud',
    collapsed: false,
  })
})

test('a folder needs a name and a long name is cut', function () {
  const store = newStore()
  expect(store.saveFolder('', '   ', 'red', 'server')).toBe(false)
  expect(store.folders).toEqual([])

  store.saveFolder('', 'x'.repeat(60), 'red', 'server')
  expect(store.folders[0].name.length).toBe(40)
})

test('an unknown colour or icon becomes the default', function () {
  const store = newStore()
  store.saveFolder('', 'Work', 'pink', 'unicorn')
  expect(store.folders[0].color).toBe('grey')
  expect(store.folders[0].icon).toBe('folder')

  // The same for saved data that this version does not know.
  raw.setItem(FOLDERS_STORAGE_KEY, JSON.stringify([{ id: 'f', name: 'Old', color: '#fff' }]))
  expect(newStore().folders).toEqual([
    { id: 'f', name: 'Old', color: 'grey', icon: 'folder', collapsed: false },
  ])
})

test('a connection moves into a folder and out again', async function () {
  const store = newStore()
  store.saveFolder('', 'Work', 'red', 'server')
  const work = idOf(store, 'Work')
  await add(store, 'a', 'Shop')

  store.move('a', work)
  expect(shape(store.sections(''))).toBe('Work[Shop] | []')
  expect(shape(newStore().sections(''))).toBe('Work[Shop] | []')

  store.move('a', '')
  expect(shape(store.sections(''))).toBe('Work[] | [Shop]')
  expect(shape(newStore().sections(''))).toBe('Work[] | [Shop]')
})

test('the folder is saved with the connection', async function () {
  const store = newStore()
  store.saveFolder('', 'Work', 'red', 'server')
  await add(store, 'a', 'Shop', idOf(store, 'Work'))
  expect(shape(newStore().sections(''))).toBe('Work[Shop] | []')
})

test('deleting a folder moves its connections to the top level and deletes none', async function () {
  const store = newStore()
  store.saveFolder('', 'Work', 'red', 'server')
  const work = idOf(store, 'Work')
  await add(store, 'a', 'Shop', work)
  await add(store, 'b', 'Lab', work)
  await add(store, 'c', 'Home')

  store.removeFolder(work)
  expect(store.folders).toEqual([])
  expect(shape(store.sections(''))).toBe('[Shop,Lab,Home]')
  expect(shape(newStore().sections(''))).toBe('[Shop,Lab,Home]')
  // The passwords stay in the keychain.
  expect(api.secrets).toEqual({ a: 'secret', b: 'secret', c: 'secret' })
})

test('a connection with an unknown folder id shows at the top level', async function () {
  const store = newStore()
  store.saveFolder('', 'Work', 'red', 'server')
  await add(store, 'a', 'Shop', 'gone')
  expect(store.folderOf(store.favorites[0])).toBe('')
  expect(shape(store.sections(''))).toBe('Work[] | [Shop]')

  store.move('a', 'gone too')
  expect(shape(store.sections(''))).toBe('Work[] | [Shop]')
})

test('old saved data without folders loads fine', function () {
  const old = [{ id: 'a', name: 'Shop', config: DEFAULT_CONFIGS.mysql }]
  raw.setItem(FAVORITES_STORAGE_KEY, JSON.stringify(old))
  const store = newStore()
  expect(store.folders).toEqual([])
  expect(shape(store.sections(''))).toBe('[Shop]')
})

test('a broken folder list gives no folders', function () {
  raw.setItem(FOLDERS_STORAGE_KEY, '{not json')
  expect(newStore().folders).toEqual([])
  raw.setItem(FOLDERS_STORAGE_KEY, JSON.stringify([null, 5, { name: 'No id' }]))
  expect(newStore().folders).toEqual([])
})

test('the closed state of a folder is saved', async function () {
  const store = newStore()
  store.saveFolder('', 'Work', 'red', 'server')
  const work = idOf(store, 'Work')
  await add(store, 'a', 'Shop', work)

  store.toggleFolder(work)
  expect(shape(store.sections(''))).toBe('Work(closed)[Shop] | []')
  expect(shape(newStore().sections(''))).toBe('Work(closed)[Shop] | []')

  store.toggleFolder(work)
  expect(shape(newStore().sections(''))).toBe('Work[Shop] | []')
})

test('folders come first, sorted by name', function () {
  const store = newStore()
  store.saveFolder('', 'Work', 'red', 'server')
  store.saveFolder('', 'Clients', 'blue', 'cloud')
  expect(shape(store.sections(''))).toBe('Clients[] | Work[] | []')
})

test('the search shows matching connections under their folder', async function () {
  const store = newStore()
  store.saveFolder('', 'Work', 'red', 'server')
  store.saveFolder('', 'Clients', 'blue', 'cloud')
  const work = idOf(store, 'Work')
  await add(store, 'a', 'Shop', work)
  await add(store, 'b', 'Lab', work)
  await add(store, 'c', 'Acme', idOf(store, 'Clients'))
  await add(store, 'd', 'Shop copy')
  store.toggleFolder(work)

  // A folder with no match is hidden. A closed folder with a match is shown open.
  expect(shape(store.sections(' SHOP '))).toBe('Work[Shop] | [Shop copy]')
  // A folder name match shows the whole folder.
  expect(shape(store.sections('work'))).toBe('Work[Shop,Lab] | []')
  expect(shape(store.sections('zzz'))).toBe('[]')
  // The search does not change the saved state.
  expect(shape(store.sections(''))).toBe('Clients[Acme] | Work(closed)[Shop,Lab] | [Shop copy]')
})

test('the dev profile has its own folders', function () {
  globalThis.__SQL_PRO_PROFILE__ = 'dev'
  newStore().saveFolder('', 'Work', 'red', 'server')
  expect(raw.items['sql-pro.folders.dev']).toBeDefined()
  expect(raw.items[FOLDERS_STORAGE_KEY]).toBeUndefined()
  expect(newStore().folders.length).toBe(1)

  // The release app does not see them.
  forgetProfile()
  expect(newStore().folders).toEqual([])
})
