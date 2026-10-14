// Checks the platform shortcuts. Run: bun test
import { expect, test } from 'bun:test'
import { Shortcuts } from '../src/services/Shortcuts'

const NONE = { metaKey: false, ctrlKey: false, shiftKey: false, altKey: false }

test('macOS uses Cmd and ignores Ctrl', function () {
  const shortcuts = new Shortcuts('MacIntel')
  expect(shortcuts.action({ ...NONE, key: 's', metaKey: true })).toBe('save')
  expect(shortcuts.action({ ...NONE, key: 'R', metaKey: true })).toBe('refresh')
  expect(shortcuts.action({ ...NONE, key: 'r', ctrlKey: true })).toBeNull()
  expect(shortcuts.action({ ...NONE, key: 's', metaKey: true, shiftKey: true })).toBeNull()
  expect(shortcuts.action({ ...NONE, key: 'x', metaKey: true })).toBeNull()
  expect(shortcuts.hint('S')).toBe('⌘S')
})

test('Windows and Linux use Ctrl and ignore the other modifiers', function () {
  for (const platform of ['Win32', 'Linux x86_64']) {
    const shortcuts = new Shortcuts(platform)
    expect(shortcuts.action({ ...NONE, key: 's', ctrlKey: true })).toBe('save')
    expect(shortcuts.action({ ...NONE, key: 'r', ctrlKey: true })).toBe('refresh')
    expect(shortcuts.action({ ...NONE, key: 's', metaKey: true })).toBeNull()
    expect(shortcuts.action({ ...NONE, key: 'r', ctrlKey: true, altKey: true })).toBeNull()
    expect(shortcuts.action({ ...NONE, key: 's' })).toBeNull()
    expect(shortcuts.hint('R')).toBe('Ctrl+R')
    // A click with Ctrl opens a table in a new tab.
    expect(shortcuts.hasModifier({ metaKey: false, ctrlKey: true })).toBe(true)
  }
})

test('Shift with the main modifier gives the export action', function () {
  const mac = new Shortcuts('MacIntel')
  // With Shift held, the key is upper case.
  expect(mac.action({ ...NONE, key: 'E', metaKey: true, shiftKey: true })).toBe('export')
  expect(mac.action({ ...NONE, key: 'e', metaKey: true })).toBeNull()
  expect(mac.action({ ...NONE, key: 'S', metaKey: true, shiftKey: true })).toBeNull()
  expect(mac.hint('E', true)).toBe('⇧⌘E')

  const windows = new Shortcuts('Win32')
  expect(windows.action({ ...NONE, key: 'E', ctrlKey: true, shiftKey: true })).toBe('export')
  expect(
    windows.action({ ...NONE, key: 'E', ctrlKey: true, shiftKey: true, altKey: true }),
  ).toBeNull()
  expect(windows.hint('E', true)).toBe('Ctrl+Shift+E')
})
