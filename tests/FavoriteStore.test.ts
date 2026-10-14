// Checks that a keychain failure never blocks a saved connection. Run: bun test
import { beforeEach, expect, test } from 'bun:test'
import { DEFAULT_CONFIGS, DEFAULT_SSH_CONFIG, SSH_PASSWORD_SUFFIX } from '../src/constants'
import type { DatabaseApi } from '../src/services/DatabaseApi'
import { FavoriteStore } from '../src/services/FavoriteStore'
import type { Favorite } from '../src/services/FavoriteStore'
import { ProfileStorage } from '../src/services/ProfileStorage'

const DENIED = 'Platform secure storage failure: denied'

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
  /** When true, every keychain call fails. */
  broken = false
  secrets: Record<string, string> = {}

  async savePassword(id: string, password: string): Promise<void> {
    this.check()
    this.secrets[id] = password
  }

  async loadPassword(id: string): Promise<string> {
    this.check()
    return this.secrets[id] ?? ''
  }

  async deletePassword(id: string): Promise<void> {
    this.check()
    delete this.secrets[id]
  }

  private check(): void {
    if (this.broken) {
      throw DENIED
    }
  }
}

function favorite(): Favorite {
  const ssh = { ...DEFAULT_SSH_CONFIG, password: 'tunnel' }
  return {
    id: 'a',
    name: 'Shop',
    environment: 'local',
    config: { ...DEFAULT_CONFIGS.mysql, password: 'secret', ssh },
  }
}

function storeOf(api: FakeApi): FavoriteStore {
  return new FavoriteStore(api as unknown as DatabaseApi, new ProfileStorage(localStorage, ''))
}

beforeEach(useFreshStorage)

function useFreshStorage(): void {
  globalThis.localStorage = new FakeStorage() as unknown as Storage
}

test('save and load give the passwords back', async function () {
  const api = new FakeApi()
  const store = storeOf(api)
  expect(await store.save(favorite())).toBe('')
  expect(store.favorites[0].config.password).toBe('')
  expect(store.favorites[0].config.ssh?.password).toBe('')

  const loaded = await store.fullConfig(store.favorites[0])
  expect(loaded.keychainError).toBe('')
  expect(loaded.config.password).toBe('secret')
  expect(loaded.config.ssh?.password).toBe('tunnel')
})

test('load with a failing keychain gives empty passwords and reports it', async function () {
  const api = new FakeApi()
  const store = storeOf(api)
  await store.save(favorite())
  api.broken = true

  const loaded = await store.fullConfig(store.favorites[0])
  expect(loaded.keychainError).toBe(DENIED)
  expect(loaded.config.password).toBe('')
  expect(loaded.config.ssh?.password).toBe('')
  expect(loaded.config.host).toBe(DEFAULT_CONFIGS.mysql.host)
})

test('load with no saved password is not a failure', async function () {
  const store = storeOf(new FakeApi())
  const saved = favorite()
  saved.config.password = ''
  saved.config.ssh = null
  await store.save(saved)

  const loaded = await store.fullConfig(store.favorites[0])
  expect(loaded.keychainError).toBe('')
  expect(loaded.config.password).toBe('')
})

test('save with a failing keychain keeps the favorite and reports it', async function () {
  const api = new FakeApi()
  api.broken = true
  const store = storeOf(api)

  expect(await store.save(favorite())).toBe(DENIED)
  expect(store.favorites.length).toBe(1)
  // A new store reads the list back from the storage.
  expect(storeOf(api).favorites[0].name).toBe('Shop')
})

test('remove with a failing keychain still removes the favorite', async function () {
  const api = new FakeApi()
  const store = storeOf(api)
  await store.save(favorite())
  api.broken = true

  expect(await store.remove('a')).toBe(DENIED)
  expect(store.favorites).toEqual([])
  expect(storeOf(api).favorites).toEqual([])
  // The entries that stay behind do no harm.
  expect(api.secrets['a' + SSH_PASSWORD_SUFFIX]).toBe('tunnel')
})

test('duplicate makes a copy with a new id and the same passwords', async function () {
  const api = new FakeApi()
  const store = storeOf(api)
  store.saveFolder('', 'Work', '', '')
  const folderId = store.folders[0].id
  await store.save({ ...favorite(), environment: 'prod', folderId })
  await store.save({ ...favorite(), id: 'b', name: 'Other' })

  const made = await store.duplicate(store.favorites[0])
  const copy = made.favorite
  expect(made.keychainError).toBe('')
  expect(copy.id).not.toBe('a')
  expect(copy.name).toBe('Shop copy')
  expect(copy.environment).toBe('prod')
  expect(copy.folderId).toBe(folderId)
  expect(copy.config).toEqual(store.favorites[0].config)
  // The copy sits right after the original.
  expect(store.favorites[1]).toBe(copy)
  expect(store.favorites.length).toBe(3)

  const loaded = await store.fullConfig(copy)
  expect(loaded.config.password).toBe('secret')
  expect(loaded.config.ssh?.password).toBe('tunnel')

  // The original is unchanged.
  expect(store.favorites[0].id).toBe('a')
  expect(store.favorites[0].name).toBe('Shop')
  expect(api.secrets.a).toBe('secret')
  expect(api.secrets['a' + SSH_PASSWORD_SUFFIX]).toBe('tunnel')
  // A new store reads the copy back from the storage.
  expect(storeOf(api).favorites[1].name).toBe('Shop copy')
})

test('duplicate picks the next free name', async function () {
  const store = storeOf(new FakeApi())
  await store.save(favorite())

  expect((await store.duplicate(store.favorites[0])).favorite.name).toBe('Shop copy')
  expect((await store.duplicate(store.favorites[0])).favorite.name).toBe('Shop copy 2')
  expect((await store.duplicate(store.favorites[0])).favorite.name).toBe('Shop copy 3')
})

test('duplicate with a failing keychain still makes the copy and reports it', async function () {
  const api = new FakeApi()
  const store = storeOf(api)
  await store.save(favorite())
  api.broken = true

  const made = await store.duplicate(store.favorites[0])
  expect(made.keychainError).toBe(DENIED)
  expect(made.favorite.name).toBe('Shop copy')
  expect(store.favorites.length).toBe(2)
  expect(api.secrets[made.favorite.id]).toBeUndefined()
})

test('byId finds a saved connection and gives undefined for an unknown id', async function () {
  const store = storeOf(new FakeApi())
  await store.save(favorite())

  expect(store.byId('a')?.name).toBe('Shop')
  expect(store.byId('missing')).toBeUndefined()
  expect(store.byId('')).toBeUndefined()
})

/** The list as a short text, like `Work[Shop,Lab] | [Home]`. */
function order(store: FavoriteStore): string {
  const parts: string[] = []
  for (const section of store.sections('')) {
    const names: string[] = []
    for (const item of section.favorites) {
      names.push(item.name)
    }
    parts.push(`${section.folder?.name ?? ''}[${names.join(',')}]`)
  }
  return parts.join(' | ')
}

/** A store with the folders Work (Shop, Lab, Blog) and Home (Pi), and Notes at the top level. */
async function sortedStore(api: FakeApi): Promise<FavoriteStore> {
  const store = storeOf(api)
  store.saveFolder('', 'Work', '', '')
  store.saveFolder('', 'Home', '', '')
  const work = store.folders[0].id
  await store.save({ ...favorite(), id: 'shop', name: 'Shop', folderId: work })
  await store.save({ ...favorite(), id: 'notes', name: 'Notes' })
  await store.save({ ...favorite(), id: 'lab', name: 'Lab', folderId: work })
  await store.save({ ...favorite(), id: 'pi', name: 'Pi', folderId: store.folders[1].id })
  await store.save({ ...favorite(), id: 'blog', name: 'Blog', folderId: work })
  return store
}

test('place reorders a connection inside its folder', async function () {
  const store = await sortedStore(new FakeApi())
  const work = store.folders[0].id
  expect(order(store)).toBe('Work[Shop,Lab,Blog] | Home[Pi] | [Notes]')

  store.place('blog', work, 'shop')
  expect(order(store)).toBe('Work[Blog,Shop,Lab] | Home[Pi] | [Notes]')
  store.place('blog', work, 'lab')
  expect(order(store)).toBe('Work[Shop,Blog,Lab] | Home[Pi] | [Notes]')
  // '' and an unknown id mean the end of the folder.
  store.place('shop', work, '')
  expect(order(store)).toBe('Work[Blog,Lab,Shop] | Home[Pi] | [Notes]')
  store.place('blog', work, 'missing')
  expect(order(store)).toBe('Work[Lab,Shop,Blog] | Home[Pi] | [Notes]')
})

test('place moves a connection to another folder at a position', async function () {
  const store = await sortedStore(new FakeApi())
  const work = store.folders[0].id
  const home = store.folders[1].id

  store.place('lab', home, 'pi')
  expect(order(store)).toBe('Work[Shop,Blog] | Home[Lab,Pi] | [Notes]')
  store.place('notes', work, 'blog')
  expect(order(store)).toBe('Work[Shop,Notes,Blog] | Home[Lab,Pi] | []')
  // To the top level, at the end. An unknown folder is the top level too.
  store.place('shop', '', '')
  store.place('pi', 'missing', '')
  expect(order(store)).toBe('Work[Notes,Blog] | Home[Lab] | [Shop,Pi]')
  // The old `move` puts it at the end of the folder.
  store.move('shop', home)
  expect(order(store)).toBe('Work[Notes,Blog] | Home[Lab,Shop] | [Pi]')
})

test('placeFolder reorders the folders', async function () {
  const store = await sortedStore(new FakeApi())
  store.saveFolder('', 'Clients', '', '')
  const [work, home, clients] = [store.folders[0].id, store.folders[1].id, store.folders[2].id]

  store.placeFolder(clients, work)
  expect(order(store)).toBe('Clients[] | Work[Shop,Lab,Blog] | Home[Pi] | [Notes]')
  store.placeFolder(clients, home)
  expect(order(store)).toBe('Work[Shop,Lab,Blog] | Clients[] | Home[Pi] | [Notes]')
  store.placeFolder(work, '')
  expect(order(store)).toBe('Clients[] | Home[Pi] | Work[Shop,Lab,Blog] | [Notes]')
})

test('the order survives a reload from the storage', async function () {
  const api = new FakeApi()
  const store = await sortedStore(api)
  store.place('blog', store.folders[0].id, 'shop')
  store.placeFolder(store.folders[1].id, store.folders[0].id)

  expect(order(storeOf(api))).toBe('Home[Pi] | Work[Blog,Shop,Lab] | [Notes]')
})

test('a drop on its own place and unknown ids change nothing and save nothing', async function () {
  const store = await sortedStore(new FakeApi())
  const work = store.folders[0].id
  const home = store.folders[1].id
  const saved = JSON.stringify((localStorage as unknown as FakeStorage).items)

  // Before itself, before the row after it, and the last one to the end.
  store.place('lab', work, 'lab')
  store.place('lab', work, 'blog')
  store.place('blog', work, '')
  store.place('missing', work, 'shop')
  store.placeFolder(work, work)
  store.placeFolder(work, home)
  store.placeFolder(home, '')
  store.placeFolder('missing', work)

  expect(order(store)).toBe('Work[Shop,Lab,Blog] | Home[Pi] | [Notes]')
  expect(JSON.stringify((localStorage as unknown as FakeStorage).items)).toBe(saved)
})
