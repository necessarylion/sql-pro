import type { SettingsSnapshot } from '../constants/settings'
import type { DatabaseApi } from './DatabaseApi'
import type { FontService } from './FontService'
import type { ThemeService } from './ThemeService'

/**
 * Opens the Settings window and keeps the settings the same in all windows.
 *
 * The Settings window saves a change and then sends all settings in an event.
 * The main window shows them at once. It does not read them again from localStorage:
 * in WKWebView each window has its own web process, and a write may reach the
 * other window a little later. The `storage` event has the same delay.
 */
export class SettingsWindow {
  private api: DatabaseApi
  private theme: ThemeService
  private fonts: FontService
  /** The content box of the Settings page. Set by `reveal`. */
  private content: HTMLElement | null = null

  constructor(api: DatabaseApi, theme: ThemeService, fonts: FontService) {
    this.api = api
    this.theme = theme
    this.fonts = fonts
  }

  /** Opens the window, or brings it to the front when it is open. */
  open(): Promise<void> {
    return this.api.openSettingsWindow()
  }

  /**
   * The Settings window calls this once, after its page is mounted.
   * The window starts hidden. It gets the height of the content first, so it shows with no jump.
   * After that it follows the content, for example when the font size changes.
   * `content` must not depend on the window height, or measuring and resizing feed each other.
   */
  async reveal(content: HTMLElement): Promise<void> {
    this.content = content
    // The web fonts change the height of the text. Measure after they are in.
    // ponytail: no wait for an animation frame. A hidden window may never draw one,
    // and `getBoundingClientRect` lays the page out by itself.
    await document.fonts.ready
    // Rust sets the height and shows the window.
    await this.fit()
    new ResizeObserver(this.onResize.bind(this)).observe(content)
  }

  /** The height of the box includes its padding. Rust adds the limits and the fixed width. */
  private fit(): Promise<void> {
    if (!this.content) {
      return Promise.resolve()
    }
    return this.api.fitSettingsWindow(this.content.getBoundingClientRect().height, innerHeight)
  }

  private onResize(): void {
    this.fit().catch(console.error)
  }

  /** Closes the Settings window. Only its own page calls this. */
  close(): Promise<void> {
    return this.api.closeWindow()
  }

  /** The Settings window calls this after each change. */
  publish(): Promise<void> {
    return this.api.emitSettings(this.snapshot())
  }

  /** The main window calls this once. It then follows each change. */
  async follow(): Promise<void> {
    await this.api.onSettingsChanged(this.receive.bind(this))
  }

  snapshot(): SettingsSnapshot {
    return {
      theme: this.theme.choice,
      uiFont: this.fonts.uiFont,
      monoFont: this.fonts.monoFont,
      size: this.fonts.size,
    }
  }

  /** Shows the settings of the other window. They are saved already, so this does not save them again. */
  receive(settings: SettingsSnapshot): void {
    this.theme.choice = settings.theme
    this.theme.apply()
    this.fonts.uiFont = settings.uiFont
    this.fonts.monoFont = settings.monoFont
    this.fonts.size = settings.size
    this.fonts.apply()
  }
}
