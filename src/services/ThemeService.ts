import {
  DEFAULT_THEME,
  THEME_LABELS,
  THEME_STORAGE_KEY,
  type Theme,
  type ThemeChoice,
} from '../constants/theme'
import type { WindowTheme } from './WindowTheme'

/** Keeps the chosen theme and puts it on the page and on the native window. */
export class ThemeService {
  choice: ThemeChoice = DEFAULT_THEME
  private storage: Storage
  private windowTheme: WindowTheme
  private media: MediaQueryList

  constructor(storage: Storage, windowTheme: WindowTheme, media: MediaQueryList) {
    this.storage = storage
    this.windowTheme = windowTheme
    this.media = media
    const saved = storage.getItem(THEME_STORAGE_KEY)
    if (saved !== null && saved in THEME_LABELS) {
      this.choice = saved as ThemeChoice
    }
  }

  /** Shows the theme now, and again each time the OS setting changes. */
  start(): void {
    this.media.addEventListener('change', this.apply.bind(this))
    this.apply()
  }

  choose(choice: ThemeChoice): void {
    this.choice = choice
    this.storage.setItem(THEME_STORAGE_KEY, choice)
    this.apply()
  }

  /** The look that is shown: the choice, or the OS setting for `system`. */
  effective(): Theme {
    if (this.choice !== 'system') {
      return this.choice
    }
    return this.media.matches ? 'light' : 'dark'
  }

  apply(): void {
    document.documentElement.dataset.theme = this.effective()
    void this.windowTheme.set(this.choice === 'system' ? null : this.choice)
  }
}
