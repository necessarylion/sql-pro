import { getCurrentWebview } from '@tauri-apps/api/webview'

/** Zooms the whole page of the native window, like the zoom of a browser. */
export class WebviewZoom {
  /** `factor` is 1 for 100%. Gives false when it fails, for example in a plain browser. */
  async set(factor: number): Promise<boolean> {
    try {
      await getCurrentWebview().setZoom(factor)
      return true
    } catch {
      return false
    }
  }
}
