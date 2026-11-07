// Checks how the Settings window sends its settings and how the main window shows them. Run: bun test
import { expect, test } from 'bun:test'
import { FONT_SIZE_DEFAULT, FONT_SIZE_STORAGE_KEY, UI_FONT_PROPERTY } from '../src/constants/fonts'
import type { SettingsSnapshot } from '../src/constants/settings'
import type { Theme } from '../src/constants/theme'
import type { DatabaseApi } from '../src/services/DatabaseApi'
import { FontService } from '../src/services/FontService'
import { ProfileStorage } from '../src/services/ProfileStorage'
import { SettingsWindow } from '../src/services/SettingsWindow'
import { ThemeService } from '../src/services/ThemeService'
import type { WindowTheme } from '../src/services/WindowTheme'

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

class FakeStyle {
  properties: Record<string, string> = {}

  setProperty(name: string, value: string): void {
    this.properties[name] = value
  }

  removeProperty(name: string): void {
    delete this.properties[name]
  }
}

/** Keeps what was sent, and hands each event to the handler that listens, like the backend. */
class FakeApi {
  sent: SettingsSnapshot[] = []
  opened = 0
  closed = 0
  private handler: ((settings: SettingsSnapshot) => void) | null = null

  async openSettingsWindow(): Promise<void> {
    this.opened += 1
  }

  async closeWindow(): Promise<void> {
    this.closed += 1
  }

  async emitSettings(settings: SettingsSnapshot): Promise<void> {
    this.sent.push(settings)
    this.handler?.(settings)
  }

  async onSettingsChanged(handler: (settings: SettingsSnapshot) => void): Promise<() => void> {
    this.handler = handler
    return function stop(): void {}
  }
}

// It has a style too: other test files load CodeMirror, which reads it.
const root = { dataset: {} as Record<string, string>, style: {} }
globalThis.document = { documentElement: root } as unknown as Document

function media(): MediaQueryList {
  return { matches: false, addEventListener(): void {} } as unknown as MediaQueryList
}

function services(api: FakeApi, storage: FakeStorage) {
  const profile = new ProfileStorage(storage as unknown as Storage, '')
  const style = new FakeStyle()
  const theme = new ThemeService(profile, new FakeWindow() as unknown as WindowTheme, media())
  const fonts = new FontService(api as unknown as DatabaseApi, profile, {
    style,
  } as unknown as HTMLElement)
  const window = new SettingsWindow(api as unknown as DatabaseApi, theme, fonts)
  return { theme, fonts, window, style }
}

test('a change in the Settings window shows at once in the main window', async function () {
  const api = new FakeApi()
  const storage = new FakeStorage()
  const main = services(api, storage)
  const settings = services(new FakeApi(), storage)
  await main.window.follow()

  settings.theme.choose('pink')
  settings.fonts.chooseUiFont('Menlo')
  settings.fonts.chooseSize(16)
  await api.emitSettings(settings.window.snapshot())

  expect(main.theme.choice).toBe('pink')
  expect(root.dataset.theme).toBe('pink')
  expect(main.fonts.uiFont).toBe('Menlo')
  expect(main.fonts.size).toBe(16)
  expect(main.style.properties[UI_FONT_PROPERTY]).toBe('"Menlo"')
})

test('publish sends all settings, and the main window does not save them again', async function () {
  const api = new FakeApi()
  const storage = new FakeStorage()
  const { window, fonts } = services(api, storage)
  fonts.chooseMonoFont('Fira Code')
  await window.publish()
  expect(api.sent).toEqual([
    { theme: 'dark', uiFont: '', monoFont: 'Fira Code', size: FONT_SIZE_DEFAULT },
  ])

  window.receive({ theme: 'light', uiFont: '', monoFont: '', size: 18 })
  expect(storage.items[FONT_SIZE_STORAGE_KEY]).toBeUndefined()
})

test('open and close go to the backend', async function () {
  const api = new FakeApi()
  const { window } = services(api, new FakeStorage())
  await window.open()
  await window.close()
  expect(api.opened).toBe(1)
  expect(api.closed).toBe(1)
})
