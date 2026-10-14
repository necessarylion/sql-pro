import {
  DETAIL_DEFAULT_WIDTH,
  DETAIL_OPEN_STORAGE_KEY,
  EDITOR_DEFAULT_HEIGHT,
  KEYS_DEFAULT_HEIGHT,
  LAYOUT_SIZES,
  SIDEBAR_DEFAULT_WIDTH,
  type LayoutSize,
} from '../constants/layout'
import type { ProfileStorage } from './ProfileStorage'

/** Keeps the sizes the user can drag and gives them to the CSS of the page. */
export class LayoutService {
  sidebarWidth = SIDEBAR_DEFAULT_WIDTH
  editorHeight = EDITOR_DEFAULT_HEIGHT
  detailWidth = DETAIL_DEFAULT_WIDTH
  keysHeight = KEYS_DEFAULT_HEIGHT
  /** True when the row detail panel of the Content view is open. One setting for all tabs. */
  detailOpen = false
  private storage: ProfileStorage
  private root: HTMLElement

  constructor(storage: ProfileStorage, root: HTMLElement) {
    this.storage = storage
    this.root = root
    for (const name of Object.keys(LAYOUT_SIZES) as LayoutSize[]) {
      // A missing or broken saved value gives the default size.
      const rule = LAYOUT_SIZES[name]
      const saved = Number(storage.getItem(rule.storageKey))
      const usable = saved >= rule.min && saved <= rule.max
      this.resize(usable ? saved : rule.fallback, name)
    }
    this.detailOpen = storage.getItem(DETAIL_OPEN_STORAGE_KEY) === '1'
  }

  /** Opens or closes the row detail panel, and saves that. */
  toggleDetail(): void {
    this.detailOpen = !this.detailOpen
    this.storage.setItem(DETAIL_OPEN_STORAGE_KEY, this.detailOpen ? '1' : '0')
  }

  /** Sets one size, kept inside its limits. Does not save it. */
  resize(value: number, name: LayoutSize = 'sidebarWidth'): void {
    const rule = LAYOUT_SIZES[name]
    this[name] = Math.round(Math.min(rule.max, Math.max(rule.min, value)))
    this.root.style.setProperty(rule.property, `${this[name]}px`)
  }

  save(name: LayoutSize = 'sidebarWidth'): void {
    this.storage.setItem(LAYOUT_SIZES[name].storageKey, String(this[name]))
  }

  reset(name: LayoutSize = 'sidebarWidth'): void {
    this.resize(LAYOUT_SIZES[name].fallback, name)
    this.save(name)
  }
}
