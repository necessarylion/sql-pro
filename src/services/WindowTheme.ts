import { getCurrentWindow } from '@tauri-apps/api/window'
import type { Theme } from '../constants/theme'

/** Tells the native window which look to use. The traffic lights and menus follow it. */
export class WindowTheme {
  /** `null` means: follow the OS. A failure is ignored, for example in a plain browser. */
  async set(theme: Theme | null): Promise<void> {
    try {
      await getCurrentWindow().setTheme(WindowTheme.native(theme))
    } catch {
      // There is no native window. The page still gets its look from CSS.
    }
  }

  /** The native window knows only light and dark. Pink has a light base, so it is light. */
  static native(theme: Theme | null): 'light' | 'dark' | null {
    return theme === 'pink' ? 'light' : theme
  }
}
