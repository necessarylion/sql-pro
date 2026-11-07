import {
  AI_RESULT_DEFAULT_HEIGHT,
  DETAIL_DEFAULT_WIDTH,
  EDITOR_DEFAULT_HEIGHT,
  KEYS_SECTION_STORAGE_KEYS,
  LAYOUT_SIZES,
  SECTION_HEIGHT_STORAGE_KEYS,
  SECTION_MAX_HEIGHT,
  SECTION_MIN_HEIGHT,
  SIDEBAR_DEFAULT_WIDTH,
  type KeysSection,
  type LayoutSize,
  type SectionHeight,
} from '../constants/layout'
import type { ProfileStorage } from './ProfileStorage'

/** Keeps the sizes the user can drag and gives them to the CSS of the page. */
export class LayoutService {
  sidebarWidth = SIDEBAR_DEFAULT_WIDTH
  editorHeight = EDITOR_DEFAULT_HEIGHT
  detailWidth = DETAIL_DEFAULT_WIDTH
  aiResultHeight = AI_RESULT_DEFAULT_HEIGHT
  /** True when the row detail panel of the Content view is open. Not saved: every start is hidden. */
  detailOpen = false
  /** True when the columns section of the Structure view is open. Saved. */
  columnsOpen = true
  /** True when the indexes section of the Structure view is open. Saved. */
  indexesOpen = true
  /** True when the foreign keys section of the Structure view is open. Saved. */
  foreignKeysOpen = true
  /** The dragged heights of the Structure view sections, in pixels. Null means as tall as the content. Saved. */
  columnsHeight: number | null = null
  indexesHeight: number | null = null
  foreignKeysHeight: number | null = null
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
    for (const name of Object.keys(KEYS_SECTION_STORAGE_KEYS) as KeysSection[]) {
      // Only a saved "0" closes a section. Anything else keeps it open.
      this[name] = storage.getItem(KEYS_SECTION_STORAGE_KEYS[name]) !== '0'
    }
    for (const name of Object.keys(SECTION_HEIGHT_STORAGE_KEYS) as SectionHeight[]) {
      // A missing or broken saved value keeps the section as tall as its content.
      const saved = Number(storage.getItem(SECTION_HEIGHT_STORAGE_KEYS[name]))
      const usable = saved >= SECTION_MIN_HEIGHT && saved <= SECTION_MAX_HEIGHT
      this[name] = usable ? Math.round(saved) : null
    }
  }

  /**
   * Sets the height of one Structure view section. Does not save it.
   * `max` is the room the other sections leave. The min height wins when the room is smaller.
   */
  resizeSection(name: SectionHeight, value: number, max = SECTION_MAX_HEIGHT): void {
    const top = Math.min(max, SECTION_MAX_HEIGHT)
    this[name] = Math.round(Math.max(SECTION_MIN_HEIGHT, Math.min(top, value)))
  }

  saveSection(name: SectionHeight): void {
    this.storage.setItem(SECTION_HEIGHT_STORAGE_KEYS[name], String(this[name] ?? ''))
  }

  /** The section is as tall as its content again. An empty saved value means that. */
  resetSection(name: SectionHeight): void {
    this[name] = null
    this.saveSection(name)
  }

  /** Opens or closes one section of the Structure view and saves it. */
  toggleSection(name: KeysSection): void {
    this[name] = !this[name]
    this.storage.setItem(KEYS_SECTION_STORAGE_KEYS[name], this[name] ? '1' : '0')
  }

  /** Opens or closes the row detail panel. */
  toggleDetail(): void {
    this.detailOpen = !this.detailOpen
  }

  /** Opens the row detail panel. Does nothing when it is open. */
  openDetail(): void {
    this.detailOpen = true
  }

  /** Hides the row detail panel. Used when a tab or a connection closes, and for a new tab. */
  closeDetail(): void {
    this.detailOpen = false
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
