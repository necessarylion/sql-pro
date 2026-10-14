// Checks that a debug app and a release app keep their saved data apart. Run: bun test
import { afterEach, expect, test } from 'bun:test'
import { DEFAULT_CONFIGS, FAVORITES_STORAGE_KEY } from '../src/constants'
import { EDITOR_HEIGHT_STORAGE_KEY, SIDEBAR_WIDTH_STORAGE_KEY } from '../src/constants/layout'
import { THEME_STORAGE_KEY } from '../src/constants/theme'
import type { DatabaseApi } from '../src/services/DatabaseApi'
import { FavoriteStore } from '../src/services/FavoriteStore'
import type { Favorite } from '../src/services/FavoriteStore'
import { LayoutService } from '../src/services/LayoutService'
import { ProfileStorage } from '../src/services/ProfileStorage'
import { ThemeService } from '../src/services/ThemeService'
import type { WindowTheme } from '../src/services/WindowTheme'

const DEV_FAVORITES_KEY = 'sql-pro.favorites.dev'

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
  async savePassword(): Promise<void> {}

  async deletePassword(): Promise<void> {}
}

class FakeRoot {
  style = { setProperty: this.setProperty.bind(this) }

  setProperty(): void {}
}

/** A storage for the profile that the global says. This is what main.ts does. */
function storageOf(raw: FakeStorage): ProfileStorage {
  return new ProfileStorage(raw as unknown as Storage, ProfileStorage.current())
}

function storeOf(raw: FakeStorage): FavoriteStore {
  return new FavoriteStore(new FakeApi() as unknown as DatabaseApi, storageOf(raw))
}

function shop(): Favorite {
  return { id: 'a', name: 'Shop', environment: 'prod', config: DEFAULT_CONFIGS.mysql }
}

afterEach(forgetProfile)

function forgetProfile(): void {
  delete globalThis.__SQL_PRO_PROFILE__
}

test('the profile is release when the global is missing or empty', function () {
  expect(ProfileStorage.current()).toBe('')
  globalThis.__SQL_PRO_PROFILE__ = ''
  expect(ProfileStorage.current()).toBe('')
  globalThis.__SQL_PRO_PROFILE__ = 'dev'
  expect(ProfileStorage.current()).toBe('dev')
})

test('the release key is the base key and the dev key is another one', function () {
  const raw = new FakeStorage() as unknown as Storage
  expect(new ProfileStorage(raw, '').key(FAVORITES_STORAGE_KEY)).toBe('sql-pro.favorites')
  expect(new ProfileStorage(raw, 'dev').key(FAVORITES_STORAGE_KEY)).toBe(DEV_FAVORITES_KEY)
})

test('the dev profile saves connections under the dev key', async function () {
  globalThis.__SQL_PRO_PROFILE__ = 'dev'
  const raw = new FakeStorage()
  await storeOf(raw).save(shop())

  expect(Object.keys(raw.items)).toEqual([DEV_FAVORITES_KEY])
  expect(storeOf(raw).favorites[0].name).toBe('Shop')
})

test('the dev profile does not load the connections of the release app', function () {
  const raw = new FakeStorage()
  raw.items[FAVORITES_STORAGE_KEY] = JSON.stringify([shop()])

  globalThis.__SQL_PRO_PROFILE__ = 'dev'
  expect(storeOf(raw).favorites).toEqual([])
})

test('the release app does not load the connections of the dev profile', function () {
  const raw = new FakeStorage()
  raw.items[DEV_FAVORITES_KEY] = JSON.stringify([shop()])

  expect(storeOf(raw).favorites).toEqual([])
})

test('the release key is used with an empty global and with no global', async function () {
  for (const profile of ['', undefined]) {
    globalThis.__SQL_PRO_PROFILE__ = profile
    const raw = new FakeStorage()
    await storeOf(raw).save(shop())

    expect(Object.keys(raw.items)).toEqual([FAVORITES_STORAGE_KEY])
    expect(storeOf(raw).favorites[0].name).toBe('Shop')
  }
})

test('the theme uses the key of the profile', function () {
  const raw = new FakeStorage()
  raw.items[THEME_STORAGE_KEY] = 'light'
  const media = { matches: false } as MediaQueryList

  globalThis.__SQL_PRO_PROFILE__ = 'dev'
  const theme = new ThemeService(storageOf(raw), {} as WindowTheme, media)
  // The light theme of the release app is not taken over.
  expect(theme.choice).toBe('dark')

  raw.items[`${THEME_STORAGE_KEY}.dev`] = 'system'
  expect(new ThemeService(storageOf(raw), {} as WindowTheme, media).choice).toBe('system')
  expect(raw.items[THEME_STORAGE_KEY]).toBe('light')
})

test('the layout uses the key of the profile', function () {
  const raw = new FakeStorage()
  raw.items[SIDEBAR_WIDTH_STORAGE_KEY] = '300'
  raw.items[EDITOR_HEIGHT_STORAGE_KEY] = '400'

  globalThis.__SQL_PRO_PROFILE__ = 'dev'
  const layout = new LayoutService(storageOf(raw), new FakeRoot() as unknown as HTMLElement)
  // The sizes of the release app are not taken over.
  expect(layout.sidebarWidth).not.toBe(300)
  expect(layout.editorHeight).not.toBe(400)

  layout.resize(250, 'sidebarWidth')
  layout.save('sidebarWidth')
  layout.resize(150, 'editorHeight')
  layout.save('editorHeight')
  expect(raw.items[`${SIDEBAR_WIDTH_STORAGE_KEY}.dev`]).toBe('250')
  expect(raw.items[`${EDITOR_HEIGHT_STORAGE_KEY}.dev`]).toBe('150')
  expect(raw.items[SIDEBAR_WIDTH_STORAGE_KEY]).toBe('300')
  expect(raw.items[EDITOR_HEIGHT_STORAGE_KEY]).toBe('400')
})
