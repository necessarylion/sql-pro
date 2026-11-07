// Checks the theme choice, with a fake storage, window and page. Run: bun test
import { expect, test } from 'bun:test'
import { THEME_LABELS, THEME_STORAGE_KEY, type Theme } from '../src/constants/theme'
import { ProfileStorage } from '../src/services/ProfileStorage'
import { ThemeService } from '../src/services/ThemeService'
import { WindowTheme } from '../src/services/WindowTheme'

class FakeStorage {
  items: Record<string, string> = {}

  getItem(key: string): string | null {
    return this.items[key] ?? null
  }

  setItem(key: string, value: string): void {
    this.items[key] = value
  }
}

class FakeWindow {
  themes: (Theme | null)[] = []

  async set(theme: Theme | null): Promise<void> {
    this.themes.push(theme)
  }
}

const root = { dataset: {} as Record<string, string> }
globalThis.document = { documentElement: root } as unknown as Document

function build(storage: FakeStorage, window = new FakeWindow()): ThemeService {
  return new ThemeService(
    new ProfileStorage(storage as unknown as Storage, ''),
    window as unknown as WindowTheme,
    { matches: true } as MediaQueryList,
  )
}

test('pink is a choice between light and system', function () {
  expect(Object.values(THEME_LABELS)).toEqual(['Dark', 'Light', 'Pink', 'System'])
})

test('choosing pink saves it and shows it on the page', function () {
  const storage = new FakeStorage()
  const window = new FakeWindow()
  const theme = build(storage, window)

  theme.choose('pink')

  expect(storage.items[THEME_STORAGE_KEY]).toBe('pink')
  expect(theme.effective()).toBe('pink')
  expect(root.dataset.theme).toBe('pink')
  expect(window.themes).toEqual(['pink'])
})

test('a saved pink choice is loaded again', function () {
  const storage = new FakeStorage()
  storage.items[THEME_STORAGE_KEY] = 'pink'

  expect(build(storage).choice).toBe('pink')
})

test('the native window shows pink as light', function () {
  expect(WindowTheme.native('pink')).toBe('light')
  expect(WindowTheme.native('dark')).toBe('dark')
  expect(WindowTheme.native(null)).toBeNull()
})
