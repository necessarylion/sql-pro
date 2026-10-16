import type { DatabaseApi } from './DatabaseApi'
import type { SessionManager } from './SessionManager'

/**
 * Opens the SQLite files that the OS gives the app ("Open With" or a double click).
 * Each file gets a tab that is connected to it.
 */
export class FileOpener {
  private api: Pick<DatabaseApi, 'takeOpenedFiles' | 'onOpenedFiles'>
  private manager: Pick<SessionManager, 'openFile'>

  constructor(
    api: Pick<DatabaseApi, 'takeOpenedFiles' | 'onOpenedFiles'>,
    manager: Pick<SessionManager, 'openFile'>,
  ) {
    this.api = api
    this.manager = manager
  }

  /**
   * Listens for files that come later, then opens the ones that came before the page loaded.
   * The event only says "there are files". The paths are always taken from the backend,
   * so a file never opens twice.
   */
  async start(): Promise<void> {
    await this.api.onOpenedFiles(this.openWaiting.bind(this))
    await this.openWaiting()
  }

  async openWaiting(): Promise<void> {
    for (const path of await this.api.takeOpenedFiles()) {
      await this.manager.openFile(path)
    }
  }
}
