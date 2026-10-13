import type { DatabaseApi } from './DatabaseApi'
import { Session } from './Session'

/** The list of open tabs. There is always at least one. */
export class SessionManager {
  sessions: Session[] = []
  activeId = ''
  private api: DatabaseApi

  constructor(api: DatabaseApi) {
    this.api = api
    this.open()
  }

  open(): void {
    const session = new Session(this.api)
    this.sessions.push(session)
    this.activeId = session.id
  }

  activate(session: Session): void {
    this.activeId = session.id
  }

  async close(session: Session): Promise<void> {
    const index = this.sessions.indexOf(session)
    if (index < 0) {
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
