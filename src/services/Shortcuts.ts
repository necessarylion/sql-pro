import {
  CANCEL_KEY,
  CANCEL_SHORTCUT_KEY,
  COPY_KEY,
  MAC_PLATFORM_PATTERN,
  OWN_ESCAPE_SELECTOR,
  SELECT_ALL_KEY,
  SHIFT_SHORTCUT_KEYS,
  SHORTCUT_KEYS,
  type ShortcutAction,
} from '../constants/shortcuts'
import { ZOOM_CODES, ZOOM_KEYS, type ZoomAction } from '../constants/zoom'

/** The keys of a key or mouse event that matter here. */
export interface ModifierKeys {
  metaKey: boolean
  ctrlKey: boolean
  shiftKey?: boolean
  altKey?: boolean
}

/** The parts of a key press that tell if Escape already had a job. */
export interface KeyTarget {
  key: string
  defaultPrevented: boolean
  /** The DOM element. Only `closest` is used, so a test can pass a fake. */
  target: EventTarget | { closest?(selector: string): unknown } | null
}

/** Knows the main modifier key of the platform: Cmd on macOS, Ctrl on Windows and Linux. */
export class Shortcuts {
  mac: boolean
  /** Key presses that a part of the page already used, like Escape that cleared the selection. */
  private claimed = new WeakSet<object>()

  /** `platform` is `navigator.platform`. */
  constructor(platform: string) {
    this.mac = MAC_PLATFORM_PATTERN.test(platform)
  }

  /** True when only the main modifier of the platform is held. Ctrl on macOS does not count. */
  hasModifier(event: ModifierKeys): boolean {
    const main = this.mac ? event.metaKey : event.ctrlKey
    const other = this.mac ? event.ctrlKey : event.metaKey
    return main && !other && !event.shiftKey && !event.altKey
  }

  /** The action of a key press, like Cmd+S or Shift+Cmd+E. null when the press is not a shortcut. */
  action(event: ModifierKeys & { key: string }): ShortcutAction | null {
    // Read each key by name. A spread copy of a real KeyboardEvent has none of them.
    const { metaKey, ctrlKey, shiftKey, altKey } = event
    if (!this.hasModifier({ metaKey, ctrlKey, altKey })) {
      return null
    }
    const keys = shiftKey ? SHIFT_SHORTCUT_KEYS : SHORTCUT_KEYS
    return keys[event.key.toLowerCase()] ?? null
  }

  /** True for Cmd+A (macOS) or Ctrl+A (Windows, Linux). Caps Lock gives 'A', so the case does not matter. */
  selectsAll(event: ModifierKeys & { key: string }): boolean {
    // Read each key by name. A spread copy of a real KeyboardEvent has none of them.
    const { metaKey, ctrlKey, shiftKey, altKey } = event
    return (
      event.key.toLowerCase() === SELECT_ALL_KEY &&
      this.hasModifier({ metaKey, ctrlKey, shiftKey, altKey })
    )
  }

  /** True for Cmd+C (macOS) or Ctrl+C (Windows, Linux). Caps Lock gives 'C', so the case does not matter. */
  copies(event: ModifierKeys & { key: string }): boolean {
    // Read each key by name. A spread copy of a real KeyboardEvent has none of them.
    const { metaKey, ctrlKey, shiftKey, altKey } = event
    return (
      event.key.toLowerCase() === COPY_KEY &&
      this.hasModifier({ metaKey, ctrlKey, shiftKey, altKey })
    )
  }

  /** True for Escape alone, or Cmd+. (macOS) or Ctrl+. (Windows, Linux). Both stop a running query. */
  cancels(event: ModifierKeys & { key: string }): boolean {
    // Read each key by name. A spread copy of a real KeyboardEvent has none of them.
    const { metaKey, ctrlKey, shiftKey, altKey } = event
    if (event.key === CANCEL_KEY) {
      return !metaKey && !ctrlKey && !shiftKey && !altKey
    }
    return (
      event.key === CANCEL_SHORTCUT_KEY && this.hasModifier({ metaKey, ctrlKey, shiftKey, altKey })
    )
  }

  /**
   * Marks a key press that already did its job. It then does not also stop a backup.
   * Not `preventDefault`: that would also keep a native dialog from closing.
   */
  claim(event: object): void {
    this.claimed.add(event)
  }

  /**
   * True when a cancel key may stop a running backup. A backup runs long, so Escape that has
   * another job here is left alone. Cmd+. / Ctrl+. has no other job, so it always counts.
   */
  cancelsBackup(event: ModifierKeys & KeyTarget): boolean {
    if (!this.cancels(event)) {
      return false
    }
    if (event.key !== CANCEL_KEY) {
      return true
    }
    if (event.defaultPrevented || this.claimed.has(event)) {
      return false
    }
    const target = event.target as { closest?(selector: string): unknown } | null
    return !target?.closest?.(OWN_ESCAPE_SELECTOR)
  }

  /**
   * The zoom action of a key press, like Cmd+= or Ctrl+-. null when the press is not one.
   * Shift is allowed, because `+` needs Shift on many keyboards.
   */
  zoom(event: ModifierKeys & { key: string; code: string }): ZoomAction | null {
    // Do not copy the event with `...event`. A real KeyboardEvent keeps its keys on the
    // prototype, so the copy has no metaKey and no ctrlKey, and no press ever matched.
    const { metaKey, ctrlKey, altKey } = event
    if (!this.hasModifier({ metaKey, ctrlKey, altKey })) {
      return null
    }
    return ZOOM_KEYS[event.key] ?? ZOOM_CODES[event.code] ?? null
  }

  /** The text that shows a shortcut, like ⌘S or Ctrl+S. With `shift`: ⇧⌘E or Ctrl+Shift+E. */
  hint(key: string, shift = false): string {
    if (this.mac) {
      return `${shift ? '⇧' : ''}⌘${key}`
    }
    return `Ctrl+${shift ? 'Shift+' : ''}${key}`
  }
}
