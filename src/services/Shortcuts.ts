import { MAC_PLATFORM_PATTERN, SHORTCUT_KEYS, type ShortcutAction } from '../constants/shortcuts'
import { ZOOM_CODES, ZOOM_KEYS, type ZoomAction } from '../constants/zoom'

/** The keys of a key or mouse event that matter here. */
export interface ModifierKeys {
  metaKey: boolean
  ctrlKey: boolean
  shiftKey?: boolean
  altKey?: boolean
}

/** Knows the main modifier key of the platform: Cmd on macOS, Ctrl on Windows and Linux. */
export class Shortcuts {
  mac: boolean

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

  /** The action of a key press, like Cmd+S. null when the press is not a shortcut. */
  action(event: ModifierKeys & { key: string }): ShortcutAction | null {
    if (!this.hasModifier(event)) {
      return null
    }
    return SHORTCUT_KEYS[event.key.toLowerCase()] ?? null
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

  /** The text that shows a shortcut, like ⌘S or Ctrl+S. */
  hint(key: string): string {
    return this.mac ? `⌘${key}` : `Ctrl+${key}`
  }
}
