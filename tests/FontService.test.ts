// Checks the font settings, with a fake backend, storage and root element. Run: bun test
import { expect, test } from 'bun:test'
import {
  FONT_SCALE_PROPERTY,
  FONT_SIZE_STORAGE_KEY,
  MONO_FONT_PROPERTY,
  UI_FONT_PROPERTY,
  UI_FONT_STORAGE_KEY,
} from '../src/constants/fonts'
import type { DatabaseApi } from '../src/services/DatabaseApi'
import { FontService } from '../src/services/FontService'
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

class FakeStyle {
  properties: Record<string, string> = {}

  setProperty(name: string, value: string): void {
    this.properties[name] = value
  }

  removeProperty(name: string): void {
    delete this.properties[name]
  }
}

class FakeApi {
  calls = 0

  async listFonts(): Promise<string[]> {
    this.calls++
    return ['Fira Code', 'Helvetica']
  }
}

function build(storage: FakeStorage, style = new FakeStyle(), api = new FakeApi()): FontService {
  return new FontService(
    api as unknown as DatabaseApi,
    new ProfileStorage(storage as unknown as Storage, ''),
    { style } as unknown as HTMLElement,
  )
}

test('picked fonts and size are put on the page, saved and restored', function () {
  const storage = new FakeStorage()
  const style = new FakeStyle()
  const fonts = build(storage, style)
  fonts.apply()
  expect(style.properties[UI_FONT_PROPERTY]).toBeUndefined()
  expect(style.properties[FONT_SCALE_PROPERTY]).toBe('1')

  fonts.chooseUiFont('Helvetica')
  fonts.chooseMonoFont('Fira Code')
  fonts.chooseSize(15)
  expect(style.properties[UI_FONT_PROPERTY]).toBe('"Helvetica"')
  expect(style.properties[MONO_FONT_PROPERTY]).toBe('"Fira Code"')
  expect(style.properties[FONT_SCALE_PROPERTY]).toBe('1.25')

  const restored = build(storage)
  expect([restored.uiFont, restored.monoFont, restored.size]).toEqual([
    'Helvetica',
    'Fira Code',
    15,
  ])

  // Default removes the pick, so the CSS default stack is used again.
  fonts.chooseUiFont('')
  expect(style.properties[UI_FONT_PROPERTY]).toBeUndefined()
  expect(storage.items[UI_FONT_STORAGE_KEY]).toBe('')
})

test('a size out of range is moved to the limit, a broken saved size gives 12', function () {
  const storage = new FakeStorage()
  const fonts = build(storage)
  fonts.chooseSize(40)
  expect(fonts.size).toBe(16)
  fonts.chooseSize(3)
  expect(fonts.size).toBe(10)
  fonts.chooseSize(Number.NaN)
  expect(fonts.size).toBe(12)

  for (const bad of ['abc', '', '99', '12.5']) {
    storage.items[FONT_SIZE_STORAGE_KEY] = bad
    expect(build(storage).size).toBe(12)
  }
})

test('the font list is asked for only once', async function () {
  const api = new FakeApi()
  const fonts = build(new FakeStorage(), new FakeStyle(), api)
  await fonts.loadFonts()
  await fonts.loadFonts()
  expect(fonts.fonts).toEqual(['Fira Code', 'Helvetica'])
  expect(api.calls).toBe(1)
})
