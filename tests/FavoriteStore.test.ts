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
