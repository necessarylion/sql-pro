import type { TableRef } from '../types'
import type { DatabaseApi } from './DatabaseApi'
import type { DiscardPrompt } from './DiscardPrompt'
import { Session } from './Session'

/** The list of open tabs. There is always at least one. */
export class SessionManager {
  sessions: Session[] = []
  activeId = ''
  private api: DatabaseApi
  private prompt: DiscardPrompt

  constructor(api: DatabaseApi, prompt: DiscardPrompt) {
    this.api = api
    this.prompt = prompt
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
  }

  activate(session: Session): void {
    this.activeId = session.id
  }

  /**
   * Opens `table` in a new tab, with its own copy of the connection of `source`.
   * The tab is added only when the copy is connected. So a failure leaves no tab behind.
   */
  async openTable(source: Session, table: TableRef): Promise<void> {
    const session = new Session(this.api, this.prompt)
    await session.cloneFrom(source)
    this.sessions.push(session)
    this.activeId = session.id
    // Read the tab back from the list. In the app that is the reactive one, so the page shows.
    const tab = this.sessions[this.sessions.length - 1]
    // A table that does not load shows its error in the new tab.
    await tab.run(tab.selectTable(table))
  }

  async close(session: Session): Promise<void> {
    const index = this.sessions.indexOf(session)
    if (index < 0 || !(await session.mayDiscard())) {
      return
    }
    this.sessions.splice(index, 1)
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
