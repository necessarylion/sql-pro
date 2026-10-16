import type { Event } from '@tauri-apps/api/event'
import { getCurrentWebview } from '@tauri-apps/api/webview'

/** A file drag over the window. `x` and `y` are CSS pixels in the page. */
export interface FileDropEvent {
  type: 'enter' | 'over' | 'drop' | 'leave'
  paths: string[]
  x: number
  y: number
}

/** Gets the file drags of the window. */
export interface FileDropTarget {
  onFileDrop(event: FileDropEvent): void
}

/** What Tauri sends. The position is in physical pixels. */
interface TauriDrop {
  type: 'enter' | 'over' | 'drop' | 'leave'
  paths?: string[]
  position?: { x: number; y: number }
}

/** Turns one Tauri drag event into a `FileDropEvent` for the target. */
class Forwarder {
  private target: FileDropTarget

  constructor(target: FileDropTarget) {
    this.target = target
  }

  handle(event: Event<TauriDrop>): void {
    const { type, paths, position } = event.payload
    // ponytail: physical pixels / devicePixelRatio. Not checked with the app's own zoom.
    const ratio = window.devicePixelRatio || 1
    this.target.onFileDrop({
      type,
      paths: paths ?? [],
      x: (position?.x ?? 0) / ratio,
      y: (position?.y ?? 0) / ratio,
    })
  }
}

/**
 * Files dragged from the desktop onto the window. Tauri catches them (`dragDropEnabled`)
 * and gives their paths. The backend reads a file by its path.
 * This is the only place that listens to these Tauri events.
 */
export class FileDrop {
  /** Starts to send file drags to `target`. Call the result to stop. */
  listen(target: FileDropTarget): Promise<() => void> {
    const forwarder = new Forwarder(target)
    return getCurrentWebview().onDragDropEvent(forwarder.handle.bind(forwarder))
  }
}
