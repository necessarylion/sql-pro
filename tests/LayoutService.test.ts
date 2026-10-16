// A check of the sidebar width and editor height logic, with a fake storage and a fake root element. Run: bun test
import { expect, test } from 'bun:test'
import {
  DETAIL_DEFAULT_WIDTH,
  DETAIL_MIN_WIDTH,
  DETAIL_WIDTH_PROPERTY,
  EDITOR_DEFAULT_HEIGHT,
  EDITOR_HEIGHT_PROPERTY,
  EDITOR_HEIGHT_STORAGE_KEY,
  EDITOR_MAX_HEIGHT,
  EDITOR_MIN_HEIGHT,
  KEYS_SECTION_STORAGE_KEYS,
  SECTION_HEIGHT_STORAGE_KEYS,
  SECTION_MIN_HEIGHT,
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

test('the detail panel starts closed, toggles, and is not saved', function () {
  const storage = new FakeStorage()
  const layout = build(storage, new FakeRoot())
  expect(layout.detailOpen).toBe(false)
  layout.toggleDetail()
  expect(layout.detailOpen).toBe(true)
  expect(Object.keys(storage.items)).toEqual([])
  storage.items['sql-pro.detail-open'] = '1'
  expect(build(storage, new FakeRoot()).detailOpen).toBe(false)
})

test('openDetail opens the panel and keeps it open, closeDetail hides it', function () {
  const layout = build(new FakeStorage(), new FakeRoot())
  layout.openDetail()
  layout.openDetail()
  expect(layout.detailOpen).toBe(true)
  layout.closeDetail()
  expect(layout.detailOpen).toBe(false)
})

test('the structure sections start open, toggle, and are saved', function () {
  const storage = new FakeStorage()
  const layout = build(storage, new FakeRoot())
  expect(layout.columnsOpen).toBe(true)
  expect(layout.indexesOpen).toBe(true)
  expect(layout.foreignKeysOpen).toBe(true)

  layout.toggleSection('indexesOpen')
  expect(layout.indexesOpen).toBe(false)
  expect(layout.foreignKeysOpen).toBe(true)
  const restored = build(storage, new FakeRoot())
  expect(restored.indexesOpen).toBe(false)
  expect(restored.foreignKeysOpen).toBe(true)

  layout.toggleSection('indexesOpen')
  expect(build(storage, new FakeRoot()).indexesOpen).toBe(true)

  // A broken saved value keeps the section open.
  storage.items[KEYS_SECTION_STORAGE_KEYS.foreignKeysOpen] = 'abc'
  expect(build(storage, new FakeRoot()).foreignKeysOpen).toBe(true)

  // The columns section is saved on its own.
  layout.toggleSection('columnsOpen')
  expect(layout.columnsOpen).toBe(false)
  expect(storage.items[KEYS_SECTION_STORAGE_KEYS.columnsOpen]).toBe('0')
  const closed = build(storage, new FakeRoot())
  expect(closed.columnsOpen).toBe(false)
  expect(closed.indexesOpen).toBe(true)
})

test('a section has no height at first, so it is as tall as its content', function () {
  const layout = build(new FakeStorage(), new FakeRoot())
  expect(layout.columnsHeight).toBeNull()
  expect(layout.indexesHeight).toBeNull()
  expect(layout.foreignKeysHeight).toBeNull()
})

test('a section height is kept inside its limits, saved, restored and reset', function () {
  const storage = new FakeStorage()
  const layout = build(storage, new FakeRoot())
  layout.resizeSection('indexesHeight', 10)
  expect(layout.indexesHeight).toBe(SECTION_MIN_HEIGHT)
  layout.resizeSection('indexesHeight', 500, 300)
  expect(layout.indexesHeight).toBe(300)
  // The min height wins when the room is too small.
  layout.resizeSection('indexesHeight', 500, 20)
  expect(layout.indexesHeight).toBe(SECTION_MIN_HEIGHT)
  layout.resizeSection('indexesHeight', 200.4)
  expect(layout.indexesHeight).toBe(200)
  expect(layout.columnsHeight).toBeNull()

  layout.saveSection('indexesHeight')
  expect(build(storage, new FakeRoot()).indexesHeight).toBe(200)

  layout.resetSection('indexesHeight')
  expect(layout.indexesHeight).toBeNull()
  expect(build(storage, new FakeRoot()).indexesHeight).toBeNull()
})

test('closing a section keeps its height', function () {
  const layout = build(new FakeStorage(), new FakeRoot())
  layout.resizeSection('columnsHeight', 200)
  layout.toggleSection('columnsOpen')
  layout.toggleSection('columnsOpen')
  expect(layout.columnsHeight).toBe(200)
})

test('a broken saved section height is ignored', function () {
  for (const bad of ['abc', '', '-5', '10', '99999', 'NaN']) {
    const storage = new FakeStorage()
    storage.items[SECTION_HEIGHT_STORAGE_KEYS.foreignKeysHeight] = bad
    expect(build(storage, new FakeRoot()).foreignKeysHeight).toBeNull()
  }
})
