// A check of the sidebar width and editor height logic, with a fake storage and a fake root element. Run: bun test
import { expect, test } from 'bun:test'
import {
  DETAIL_DEFAULT_WIDTH,
  DETAIL_MIN_WIDTH,
  DETAIL_OPEN_STORAGE_KEY,
  DETAIL_WIDTH_PROPERTY,
  EDITOR_DEFAULT_HEIGHT,
  EDITOR_HEIGHT_PROPERTY,
  EDITOR_HEIGHT_STORAGE_KEY,
  EDITOR_MAX_HEIGHT,
  EDITOR_MIN_HEIGHT,
  KEYS_DEFAULT_HEIGHT,
  KEYS_HEIGHT_PROPERTY,
  KEYS_HEIGHT_STORAGE_KEY,
  KEYS_MIN_HEIGHT,
  SIDEBAR_DEFAULT_WIDTH,
  SIDEBAR_MAX_WIDTH,
  SIDEBAR_MIN_WIDTH,
  SIDEBAR_WIDTH_PROPERTY,
  SIDEBAR_WIDTH_STORAGE_KEY,
} from '../src/constants/layout'
import { LayoutService } from '../src/services/LayoutService'
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

class FakeRoot {
  properties: Record<string, string> = {}
  style = { setProperty: this.setProperty.bind(this) }

  setProperty(name: string, value: string): void {
    this.properties[name] = value
  }
}

function build(storage: FakeStorage, root: FakeRoot): LayoutService {
  const profileStorage = new ProfileStorage(storage as unknown as Storage, '')
  return new LayoutService(profileStorage, root as unknown as HTMLElement)
}

test('resize keeps the width inside the limits and gives it to the CSS', function () {
  const root = new FakeRoot()
  const layout = build(new FakeStorage(), root)
  expect(layout.sidebarWidth).toBe(SIDEBAR_DEFAULT_WIDTH)
  expect(root.properties[SIDEBAR_WIDTH_PROPERTY]).toBe(`${SIDEBAR_DEFAULT_WIDTH}px`)

  layout.resize(10)
  expect(layout.sidebarWidth).toBe(SIDEBAR_MIN_WIDTH)
  layout.resize(5000)
  expect(layout.sidebarWidth).toBe(SIDEBAR_MAX_WIDTH)
  layout.resize(300.4)
  expect(layout.sidebarWidth).toBe(300)
  expect(root.properties[SIDEBAR_WIDTH_PROPERTY]).toBe('300px')
})

test('the width is saved, restored on start, and reset goes back to the default', function () {
  const storage = new FakeStorage()
  const layout = build(storage, new FakeRoot())
  layout.resize(300)
  layout.save()
  expect(build(storage, new FakeRoot()).sidebarWidth).toBe(300)

  layout.reset()
  expect(layout.sidebarWidth).toBe(SIDEBAR_DEFAULT_WIDTH)
  expect(storage.items[SIDEBAR_WIDTH_STORAGE_KEY]).toBe(String(SIDEBAR_DEFAULT_WIDTH))
})

test('a broken saved value gives the default width', function () {
  for (const bad of ['abc', '', '-5', '99999', 'NaN']) {
    const storage = new FakeStorage()
    storage.items[SIDEBAR_WIDTH_STORAGE_KEY] = bad
    expect(build(storage, new FakeRoot()).sidebarWidth).toBe(SIDEBAR_DEFAULT_WIDTH)
  }
})

test('the editor height is kept inside its limits, saved, restored and reset', function () {
  const storage = new FakeStorage()
  const root = new FakeRoot()
  const layout = build(storage, root)
  expect(layout.editorHeight).toBe(EDITOR_DEFAULT_HEIGHT)
  expect(root.properties[EDITOR_HEIGHT_PROPERTY]).toBe(`${EDITOR_DEFAULT_HEIGHT}px`)

  layout.resize(10, 'editorHeight')
  expect(layout.editorHeight).toBe(EDITOR_MIN_HEIGHT)
  layout.resize(99999, 'editorHeight')
  expect(layout.editorHeight).toBe(EDITOR_MAX_HEIGHT)
  layout.resize(300.4, 'editorHeight')
  expect(root.properties[EDITOR_HEIGHT_PROPERTY]).toBe('300px')
  // The sidebar width is a different value. It does not change.
  expect(layout.sidebarWidth).toBe(SIDEBAR_DEFAULT_WIDTH)

  layout.save('editorHeight')
  expect(build(storage, new FakeRoot()).editorHeight).toBe(300)

  layout.reset('editorHeight')
  expect(layout.editorHeight).toBe(EDITOR_DEFAULT_HEIGHT)
  expect(storage.items[EDITOR_HEIGHT_STORAGE_KEY]).toBe(String(EDITOR_DEFAULT_HEIGHT))
})

test('a broken saved value gives the default editor height', function () {
  for (const bad of ['abc', '', '-5', '99999', 'NaN']) {
    const storage = new FakeStorage()
    storage.items[EDITOR_HEIGHT_STORAGE_KEY] = bad
    expect(build(storage, new FakeRoot()).editorHeight).toBe(EDITOR_DEFAULT_HEIGHT)
  }
})

test('the detail panel width is kept inside its limits, saved and restored', function () {
  const storage = new FakeStorage()
  const root = new FakeRoot()
  const layout = build(storage, root)
  expect(layout.detailWidth).toBe(DETAIL_DEFAULT_WIDTH)
  expect(root.properties[DETAIL_WIDTH_PROPERTY]).toBe(`${DETAIL_DEFAULT_WIDTH}px`)
  layout.resize(10, 'detailWidth')
  expect(layout.detailWidth).toBe(DETAIL_MIN_WIDTH)
  layout.resize(400, 'detailWidth')
  layout.save('detailWidth')
  expect(build(storage, new FakeRoot()).detailWidth).toBe(400)
})

test('the detail panel starts closed, and open or closed is saved', function () {
  const storage = new FakeStorage()
  const layout = build(storage, new FakeRoot())
  expect(layout.detailOpen).toBe(false)
  layout.toggleDetail()
  expect(layout.detailOpen).toBe(true)
  expect(storage.items[DETAIL_OPEN_STORAGE_KEY]).toBe('1')
  expect(build(storage, new FakeRoot()).detailOpen).toBe(true)
  layout.toggleDetail()
  expect(build(storage, new FakeRoot()).detailOpen).toBe(false)
})

test('the keys area height is kept inside its limits, saved, restored and reset', function () {
  const storage = new FakeStorage()
  const root = new FakeRoot()
  const layout = build(storage, root)
  expect(layout.keysHeight).toBe(KEYS_DEFAULT_HEIGHT)
  expect(root.properties[KEYS_HEIGHT_PROPERTY]).toBe(`${KEYS_DEFAULT_HEIGHT}px`)
  layout.resize(5, 'keysHeight')
  expect(layout.keysHeight).toBe(KEYS_MIN_HEIGHT)
  layout.resize(250.4, 'keysHeight')
  layout.save('keysHeight')
  expect(build(storage, new FakeRoot()).keysHeight).toBe(250)
  layout.reset('keysHeight')
  expect(storage.items[KEYS_HEIGHT_STORAGE_KEY]).toBe(String(KEYS_DEFAULT_HEIGHT))
  storage.items[KEYS_HEIGHT_STORAGE_KEY] = 'abc'
  expect(build(storage, new FakeRoot()).keysHeight).toBe(KEYS_DEFAULT_HEIGHT)
})
