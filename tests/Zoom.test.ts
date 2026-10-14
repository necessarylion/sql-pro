// Checks the zoom keys and the zoom steps, with a fake storage, window and root element. Run: bun test
import { expect, test } from 'bun:test'
import { ZOOM_PROPERTY, ZOOM_STEPS, ZOOM_STORAGE_KEY } from '../src/constants/zoom'
import { ProfileStorage } from '../src/services/ProfileStorage'
import { Shortcuts } from '../src/services/Shortcuts'
import type { WebviewZoom } from '../src/services/WebviewZoom'
import { ZoomService } from '../src/services/ZoomService'

const NONE = { metaKey: false, ctrlKey: false, shiftKey: false, altKey: false, code: '' }

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

class FakeWebviewZoom {
  factors: number[] = []
  works = true

  async set(factor: number): Promise<boolean> {
    this.factors.push(factor)
    return this.works
  }
}

function build(
  storage: FakeStorage,
  webview = new FakeWebviewZoom(),
  root = new FakeRoot(),
  profile = '',
): ZoomService {
  return new ZoomService(
    new ProfileStorage(storage as unknown as Storage, profile),
    webview as WebviewZoom,
    root as unknown as HTMLElement,
  )
}

test('macOS zooms with Cmd, Windows and Linux with Ctrl', function () {
  const mac = new Shortcuts('MacIntel')
  expect(mac.zoom({ ...NONE, key: '=', metaKey: true })).toBe('in')
  expect(mac.zoom({ ...NONE, key: '-', metaKey: true })).toBe('out')
  expect(mac.zoom({ ...NONE, key: '0', metaKey: true })).toBe('reset')
  expect(mac.zoom({ ...NONE, key: '=', ctrlKey: true })).toBeNull()

  for (const platform of ['Win32', 'Linux x86_64']) {
    const other = new Shortcuts(platform)
    expect(other.zoom({ ...NONE, key: '=', ctrlKey: true })).toBe('in')
    expect(other.zoom({ ...NONE, key: '-', ctrlKey: true })).toBe('out')
    expect(other.zoom({ ...NONE, key: '0', ctrlKey: true })).toBe('reset')
    expect(other.zoom({ ...NONE, key: '=', metaKey: true })).toBeNull()
  }
})

/** Like a real KeyboardEvent: the keys are getters on the prototype, not own properties. */
class FakeKeyEvent {
  get metaKey(): boolean {
    return true
  }
  get ctrlKey(): boolean {
    return false
  }
  get shiftKey(): boolean {
    return false
  }
  get altKey(): boolean {
    return false
  }
  get key(): string {
    return '='
  }
  get code(): string {
    return 'Equal'
  }
}

test('a real key event zooms, where the keys are not own properties', function () {
  expect(new Shortcuts('MacIntel').zoom(new FakeKeyEvent())).toBe('in')
})

test('the zoom keys work with Shift, on the numpad and on other layouts', function () {
  const shortcuts = new Shortcuts('Win32')
  const ctrl = { ...NONE, ctrlKey: true }
  expect(shortcuts.zoom({ ...ctrl, key: '+', shiftKey: true })).toBe('in')
  expect(shortcuts.zoom({ ...ctrl, key: '_', shiftKey: true })).toBe('out')
  expect(shortcuts.zoom({ ...ctrl, key: '+', code: 'NumpadAdd' })).toBe('in')
  expect(shortcuts.zoom({ ...ctrl, key: '-', code: 'NumpadSubtract' })).toBe('out')
  // NumLock is off: the key is not a digit.
  expect(shortcuts.zoom({ ...ctrl, key: 'Insert', code: 'Numpad0' })).toBe('reset')
  // A French keyboard: the digit needs Shift.
  expect(shortcuts.zoom({ ...ctrl, key: 'à', code: 'Digit0' })).toBe('reset')
  // A German keyboard: `+` is on its own key.
  expect(shortcuts.zoom({ ...ctrl, key: '+', code: 'BracketRight' })).toBe('in')
})

test('no modifier or a wrong modifier does not zoom', function () {
  const shortcuts = new Shortcuts('Win32')
  for (const key of ['=', '+', '-', '_', '0']) {
    expect(shortcuts.zoom({ ...NONE, key })).toBeNull()
    expect(shortcuts.zoom({ ...NONE, key, shiftKey: true })).toBeNull()
    expect(shortcuts.zoom({ ...NONE, key, altKey: true })).toBeNull()
    expect(shortcuts.zoom({ ...NONE, key, ctrlKey: true, altKey: true })).toBeNull()
    expect(shortcuts.zoom({ ...NONE, key, ctrlKey: true, metaKey: true })).toBeNull()
  }
  expect(shortcuts.zoom({ ...NONE, key: 's', code: 'KeyS', ctrlKey: true })).toBeNull()
  // The save and refresh keys are not changed by the zoom keys.
  expect(shortcuts.action({ ...NONE, key: 's', ctrlKey: true })).toBe('save')
  expect(shortcuts.action({ ...NONE, key: '=', ctrlKey: true })).toBeNull()
})

test('zoom goes through the steps and stops at both ends', async function () {
  const webview = new FakeWebviewZoom()
  const root = new FakeRoot()
  const zoom = build(new FakeStorage(), webview, root)
  expect(zoom.percent).toBe(100)

  const up = []
  for (let press = 0; press < 7; press++) {
    await zoom.change('in')
    up.push(zoom.percent)
  }
  expect(up).toEqual([110, 125, 150, 175, 200, 200, 200])
  expect(webview.factors.at(-1)).toBe(2)
  expect(root.properties[ZOOM_PROPERTY]).toBe('2')

  const down = []
  for (let press = 0; press < 12; press++) {
    await zoom.change('out')
    down.push(zoom.percent)
  }
  expect(down).toEqual([175, 150, 125, 110, 100, 90, 80, 75, 67, 50, 50, 50])
  expect(webview.factors.at(-1)).toBe(0.5)

  await zoom.change('reset')
  expect(zoom.percent).toBe(100)
  expect(root.properties[ZOOM_PROPERTY]).toBe('1')
})

test('the zoom is saved and restored, apart for each profile', async function () {
  const storage = new FakeStorage()
  const zoom = build(storage)
  await zoom.change('in')
  await zoom.change('in')
  expect(storage.items[ZOOM_STORAGE_KEY]).toBe('125')

  const webview = new FakeWebviewZoom()
  const restored = build(storage, webview)
  expect(restored.percent).toBe(125)
  await restored.apply()
  expect(webview.factors).toEqual([1.25])

  // The dev profile has its own key. It does not see the zoom of the release app.
  const dev = build(storage, new FakeWebviewZoom(), new FakeRoot(), 'dev')
  expect(dev.percent).toBe(100)
  await dev.change('out')
  expect(storage.items[`${ZOOM_STORAGE_KEY}.dev`]).toBe('90')
  expect(storage.items[ZOOM_STORAGE_KEY]).toBe('125')
})

test('a broken saved value gives 100%', function () {
  for (const bad of ['abc', '', '-5', '99999', 'NaN', '120', '1.25']) {
    const storage = new FakeStorage()
    storage.items[ZOOM_STORAGE_KEY] = bad
    expect(build(storage).percent).toBe(100)
  }
  expect(ZOOM_STEPS).toContain(100)
})

test('a window that cannot zoom leaves the CSS alone', async function () {
  const webview = new FakeWebviewZoom()
  webview.works = false
  const root = new FakeRoot()
  const zoom = build(new FakeStorage(), webview, root)
  await zoom.change('in')
  expect(zoom.percent).toBe(110)
  expect(root.properties[ZOOM_PROPERTY]).toBeUndefined()
})
