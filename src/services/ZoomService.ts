import {
  ZOOM_DEFAULT,
  ZOOM_PROPERTY,
  ZOOM_STEPS,
  ZOOM_STORAGE_KEY,
  type ZoomAction,
} from '../constants/zoom'
import type { ProfileStorage } from './ProfileStorage'
import type { WebviewZoom } from './WebviewZoom'

/** Keeps the zoom level of the app and puts it on the native window. */
export class ZoomService {
  /** The zoom level in percent. It is always one of ZOOM_STEPS. */
  percent = ZOOM_DEFAULT
  private storage: ProfileStorage
  private webviewZoom: WebviewZoom
  private root: HTMLElement

  constructor(storage: ProfileStorage, webviewZoom: WebviewZoom, root: HTMLElement) {
    this.storage = storage
    this.webviewZoom = webviewZoom
    this.root = root
    // A missing or broken saved value gives 100%.
    const saved = Number(storage.getItem(ZOOM_STORAGE_KEY))
    if (ZOOM_STEPS.includes(saved)) {
      this.percent = saved
    }
  }

  /** One step in or out, or back to 100%. It stops at the smallest and the largest step. */
  async change(action: ZoomAction): Promise<void> {
    const index = ZOOM_STEPS.indexOf(this.percent) + (action === 'in' ? 1 : -1)
    this.percent =
      action === 'reset'
        ? ZOOM_DEFAULT
        : ZOOM_STEPS[Math.min(ZOOM_STEPS.length - 1, Math.max(0, index))]
    this.storage.setItem(ZOOM_STORAGE_KEY, String(this.percent))
    await this.apply()
  }

  /**
   * Zooms the window. The CSS gets the factor only when the window really zoomed.
   * The window buttons of macOS do not zoom, so the CSS needs the factor to keep room for them.
   */
  async apply(): Promise<void> {
    if (await this.webviewZoom.set(this.percent / 100)) {
      // Read the level again. A newer key press may have changed it while this one waited.
      this.root.style.setProperty(ZOOM_PROPERTY, String(this.percent / 100))
    }
  }
}
