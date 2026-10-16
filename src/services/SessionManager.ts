import type { TableRef } from '../types'
import type { DatabaseApi } from './DatabaseApi'
import type { DiscardPrompt } from './DiscardPrompt'
import type { LayoutService } from './LayoutService'
import { Session } from './Session'

/** The list of open tabs. There is always at least one. */
export class SessionManager {
  sessions: Session[] = []
  activeId = ''
  private api: DatabaseApi
  private prompt: DiscardPrompt
  private layout: Pick<LayoutService, 'closeDetail'>

  constructor(api: DatabaseApi, prompt: DiscardPrompt, layout: Pick<LayoutService, 'closeDetail'>) {
    this.api = api
    this.prompt = prompt
    this.layout = layout
    this.open()
  }

  /** The tab that is showing. */
  get active(): Session | undefined {
    for (const session of this.sessions) {
      if (session.id === this.activeId) {
        return session
      }
    }
    return undefined
  }

  open(): void {
    const session = new Session(this.api, this.prompt)
    this.sessions.push(session)
    this.activeId = session.id
    this.layout.closeDetail()
  }

  activate(session: Session): void {
    this.activeId = session.id
  }

  /**
   * Opens `table` in a new tab, with its own copy of the connection of `source`.
   * The tab shows at once with a loading state. The copy is fast when the backend has a spare
   * connection ready, but it can be slow when it must open a new one to a far away database.
   * When the copy fails, the tab goes away and the error shows in `source`.
   */
  async openTable(source: Session, table: TableRef): Promise<void> {
    this.sessions.push(new Session(this.api, this.prompt))
    // Read the tab back from the list. In the app that is the reactive one, so the page shows.
    const tab = this.sessions[this.sessions.length - 1]
    tab.title = source.title
    tab.opening = true
    this.activeId = tab.id
    this.layout.closeDetail()
    try {
      await tab.cloneFrom(source)
    } catch (error) {
      const showing = this.activeId === tab.id
      await this.close(tab)
      if (showing && this.sessions.includes(source)) {
        this.activeId = source.id
      }
      throw error
    } finally {
      tab.opening = false
    }
    if (!this.sessions.includes(tab)) {
      // The user closed the tab while it was loading. Close its connection too.
      await tab.disconnect()
      return
    }
    // A table that does not load shows its error in the new tab.
    await tab.run(tab.selectTable(table))
  }

  async close(session: Session): Promise<void> {
    const index = this.sessions.indexOf(session)
    if (index < 0 || !(await session.mayDiscard())) {
      return
    }
    this.sessions.splice(index, 1)
    this.layout.closeDetail()
    if (this.sessions.length === 0) {
      this.open()
    } else if (this.activeId === session.id) {
      this.activeId = this.sessions[Math.max(0, index - 1)].id
    }
    if (session.config) {
      await session.disconnect()
    }
  }
}
