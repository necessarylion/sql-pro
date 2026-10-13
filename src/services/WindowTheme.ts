import { getCurrentWindow } from '@tauri-apps/api/window'
import type { Theme } from '../constants/theme'

/** Tells the native window which look to use. The traffic lights and menus follow it. */
export class WindowTheme {
  /** `null` means: follow the OS. A failure is ignored, for example in a plain browser. */
  async set(theme: Theme | null): Promise<void> {
    try {
      await getCurrentWindow().setTheme(theme)
    } catch {
      // There is no native window. The page still gets its look from CSS.
    }
  }
}
