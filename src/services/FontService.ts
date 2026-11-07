import {
  FONT_SCALE_PROPERTY,
  FONT_SIZE_DEFAULT,
  FONT_SIZE_MAX,
  FONT_SIZE_MIN,
  FONT_SIZE_STORAGE_KEY,
  MONO_FONT_PROPERTY,
  MONO_FONT_STORAGE_KEY,
  UI_FONT_PROPERTY,
  UI_FONT_STORAGE_KEY,
} from '../constants/fonts'
import type { DatabaseApi } from './DatabaseApi'
import type { ProfileStorage } from './ProfileStorage'

/**
 * Keeps the chosen fonts and font size and puts them on the page.
 * An empty font name means the default font of the app.
 */
export class FontService {
  uiFont = ''
  monoFont = ''
  size = FONT_SIZE_DEFAULT
  /** The installed fonts. Empty until `loadFonts` ran. */
  fonts: string[] = []
  private api: DatabaseApi
  private storage: ProfileStorage
  private root: HTMLElement

  constructor(api: DatabaseApi, storage: ProfileStorage, root: HTMLElement) {
    this.api = api
    this.storage = storage
    this.root = root
    this.uiFont = storage.getItem(UI_FONT_STORAGE_KEY) ?? ''
    this.monoFont = storage.getItem(MONO_FONT_STORAGE_KEY) ?? ''
    // A missing or broken saved size gives the default.
    const saved = Number(storage.getItem(FONT_SIZE_STORAGE_KEY))
    if (Number.isInteger(saved) && saved >= FONT_SIZE_MIN && saved <= FONT_SIZE_MAX) {
      this.size = saved
    }
  }

  /** Asks the backend for the installed fonts once. The scan of the disk is slow. */
  async loadFonts(): Promise<void> {
    if (this.fonts.length === 0) {
      this.fonts = await this.api.listFonts()
    }
  }

  chooseUiFont(name: string): void {
    this.uiFont = name
    this.storage.setItem(UI_FONT_STORAGE_KEY, name)
    this.apply()
  }

  chooseMonoFont(name: string): void {
    this.monoFont = name
    this.storage.setItem(MONO_FONT_STORAGE_KEY, name)
    this.apply()
  }

  /** A size out of range is moved to the nearest limit. */
  chooseSize(size: number): void {
    const whole = Number.isFinite(size) ? Math.round(size) : FONT_SIZE_DEFAULT
    this.size = Math.min(FONT_SIZE_MAX, Math.max(FONT_SIZE_MIN, whole))
    this.storage.setItem(FONT_SIZE_STORAGE_KEY, String(this.size))
    this.apply()
  }

  apply(): void {
    this.setFont(UI_FONT_PROPERTY, this.uiFont)
    this.setFont(MONO_FONT_PROPERTY, this.monoFont)
    this.root.style.setProperty(FONT_SCALE_PROPERTY, String(this.size / FONT_SIZE_DEFAULT))
  }

  /** The name goes in quotes, so a name with spaces works. Without a name, CSS uses the default. */
  private setFont(property: string, name: string): void {
    if (name === '') {
      this.root.style.removeProperty(property)
    } else {
      this.root.style.setProperty(property, JSON.stringify(name))
    }
  }
}
