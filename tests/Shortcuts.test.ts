// Checks the platform shortcuts. Run: bun test
import { expect, test } from 'bun:test'
import { Shortcuts } from '../src/services/Shortcuts'

const NONE = { metaKey: false, ctrlKey: false, shiftKey: false, altKey: false }

test('macOS uses Cmd and ignores Ctrl', function () {
  const shortcuts = new Shortcuts('MacIntel')
  expect(shortcuts.action({ ...NONE, key: 's', metaKey: true })).toBe('save')
  expect(shortcuts.action({ ...NONE, key: 'R', metaKey: true })).toBe('refresh')
  expect(shortcuts.action({ ...NONE, key: 'i', metaKey: true })).toBe('detail')
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

test('Escape alone and the main modifier with a dot cancel a query', function () {
  const mac = new Shortcuts('MacIntel')
  expect(mac.cancels({ ...NONE, key: 'Escape' })).toBe(true)
  expect(mac.cancels({ ...NONE, key: 'Escape', metaKey: true })).toBe(false)
  expect(mac.cancels({ ...NONE, key: '.', metaKey: true })).toBe(true)
  expect(mac.cancels({ ...NONE, key: '.', ctrlKey: true })).toBe(false)
  expect(mac.cancels({ ...NONE, key: '.' })).toBe(false)
  // Cancel is not one of the save, refresh or export actions.
  expect(mac.action({ ...NONE, key: '.', metaKey: true })).toBeNull()

  const windows = new Shortcuts('Win32')
  expect(windows.cancels({ ...NONE, key: '.', ctrlKey: true })).toBe(true)
  expect(windows.cancels({ ...NONE, key: '.', metaKey: true })).toBe(false)
  expect(windows.cancels({ ...NONE, key: 'Escape', shiftKey: true })).toBe(false)
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

test('Cmd+A on macOS and Ctrl+A on Windows and Linux select all rows', function () {
  const mac = new Shortcuts('MacIntel')
  expect(mac.selectsAll({ ...NONE, key: 'a', metaKey: true })).toBe(true)
  // Caps Lock gives a capital letter.
  expect(mac.selectsAll({ ...NONE, key: 'A', metaKey: true })).toBe(true)
  expect(mac.selectsAll({ ...NONE, key: 'a', ctrlKey: true })).toBe(false)
  expect(mac.selectsAll({ ...NONE, key: 'a', metaKey: true, shiftKey: true })).toBe(false)
  expect(mac.selectsAll({ ...NONE, key: 'a' })).toBe(false)
  const windows = new Shortcuts('Win32')
  expect(windows.selectsAll({ ...NONE, key: 'a', ctrlKey: true })).toBe(true)
  expect(windows.selectsAll({ ...NONE, key: 'a', metaKey: true })).toBe(false)
})
