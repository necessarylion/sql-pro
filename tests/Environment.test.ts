// Checks the environment name of a connection: cleaning, colour, saving. Run: bun test
import { beforeEach, expect, test } from 'bun:test'
import { DEFAULT_CONFIGS, FAVORITES_STORAGE_KEY } from '../src/constants'
import { ENVIRONMENT_KINDS } from '../src/constants/environment'
import type { DatabaseApi } from '../src/services/DatabaseApi'
import { Environment } from '../src/services/Environment'
import { FavoriteStore } from '../src/services/FavoriteStore'
import { ProfileStorage } from '../src/services/ProfileStorage'
import { Session } from '../src/services/Session'
import { FakePrompt } from './FakePrompt'
import type { TableRef, UserList } from '../src/types'

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
  async connect(): Promise<void> {}

  async disconnect(): Promise<void> {}

  async listDatabases(): Promise<string[]> {
    return []
  }

  async listTables(): Promise<TableRef[]> {
    return []
  }

  async listUsers(): Promise<UserList> {
    return { canManage: false, users: [] }
  }

  async savePassword(): Promise<void> {}

  async deletePassword(): Promise<void> {}
}

function fakeApi(): DatabaseApi {
  return new FakeApi() as unknown as DatabaseApi
}

function newStore(): FavoriteStore {
  return new FavoriteStore(fakeApi(), new ProfileStorage(localStorage, ''))
}

beforeEach(useFreshStorage)

function useFreshStorage(): void {
  globalThis.localStorage = new FakeStorage() as unknown as Storage
}

test('a typed name is made lower case and clean', function () {
  expect(Environment.clean('Staging')).toBe('staging')
  expect(Environment.clean('  my env  ')).toBe('myenv')
  expect(Environment.clean('uat#1!/é')).toBe('uat1')
  expect(Environment.clean('eu-west_2')).toBe('eu-west_2')
  expect(Environment.clean('abcdefghijklmnopqrstuvwxyz')).toBe('abcdefghijklmnop')
})

test('an empty name cleans to empty, and `orLocal` makes it local', function () {
  expect(Environment.clean('')).toBe('')
  expect(Environment.clean('   ')).toBe('')
  expect(Environment.clean('!!!')).toBe('')
  expect(Environment.clean(undefined)).toBe('')
  expect(Environment.orLocal('')).toBe('local')
  expect(Environment.orLocal('!!!')).toBe('local')
  expect(Environment.orLocal(undefined)).toBe('local')
  expect(Environment.orLocal('Prod')).toBe('prod')
})

test('a built-in name typed as a custom name is the built-in one', function () {
  expect(Environment.clean(' Prod ')).toBe('prod')
  expect(Environment.kind(' Prod ')).toBe('prod')
  expect(Environment.kind('Local')).toBe('local')
})

test('each name gets its colour kind', function () {
  expect(Environment.kind('local')).toBe('local')
  expect(Environment.kind('dev')).toBe('dev')
  expect(Environment.kind('qa')).toBe('qa')
  expect(Environment.kind('prod')).toBe('prod')
  expect(Environment.kind('staging')).toBe('custom')
  expect(Environment.kind('production')).toBe('custom')
  expect(Environment.kind('localhost')).toBe('custom')
  expect(Environment.kind('')).toBe('none')
})

test('the switch of the form has the built-in names in order', function () {
  expect(ENVIRONMENT_KINDS).toEqual(['local', 'dev', 'qa', 'prod', 'custom'])
})

test('a saved connection with an empty environment is saved as local', async function () {
  const store = newStore()
  await store.save({ id: 'a', name: 'Shop', environment: '', config: DEFAULT_CONFIGS.mysql })
  expect(newStore().favorites[0].environment).toBe('local')
})

test('a saved connection keeps its environment through save and load', async function () {
  const store = newStore()
  await store.save({ id: 'a', name: 'Shop', environment: 'QA', config: DEFAULT_CONFIGS.mysql })
  await store.save({ id: 'b', name: 'Lab', environment: 'staging', config: DEFAULT_CONFIGS.mysql })

  const loaded = newStore()
  expect(loaded.favorites[0].environment).toBe('qa')
  expect(loaded.favorites[1].environment).toBe('staging')
})

test('an old saved connection without the field loads as local', function () {
  const old = [{ id: 'a', name: 'Shop', config: DEFAULT_CONFIGS.mysql }]
  localStorage.setItem(FAVORITES_STORAGE_KEY, JSON.stringify(old))

  const store = newStore()
  expect(store.favorites[0].name).toBe('Shop')
  expect(store.favorites[0].environment).toBe('local')
})

test('the session carries the environment while it is connected', async function () {
  const session = new Session(fakeApi(), new FakePrompt())
  expect(session.environment).toBe('')

  await session.connect(DEFAULT_CONFIGS.mysql, 'Shop', 'prod')
  expect(session.environment).toBe('prod')
  // The environment is not sent to the backend inside the config.
  expect(session.config).not.toHaveProperty('environment')

  await session.disconnect()
  expect(session.environment).toBe('')
})

test('a connection without an environment has none', async function () {
  const session = new Session(fakeApi(), new FakePrompt())
  await session.connect(DEFAULT_CONFIGS.mysql, 'Shop')
  expect(session.environment).toBe('')
})
