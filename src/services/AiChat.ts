import { markRaw } from 'vue'
import {
  AI_CELL_MAX,
  AI_CONFIRM_TITLE,
  AI_DEFAULT_PROVIDER,
  AI_MAX_STEPS,
  AI_READ_ONLY_REJECTED,
  AI_REJECTED,
  AI_RESULT_ROWS,
  AI_SESSION_GONE,
  AI_TITLE_MAX,
} from '../constants/ai'
import type { AiOptions, ChatEntry, ClaudeReply, ResultSet, SavedChat } from '../types'
import type { DatabaseApi } from './DatabaseApi'
import type { DiscardPrompt } from './DiscardPrompt'

/**
 * The Ask AI chat of one tab. The app drives the loop: the AI sends one SQL
 * statement, the app runs it and sends the result back, until the AI is done.
 * Today the AI is Claude Code. The loop itself does not depend on it.
 * A read runs at once. Anything else asks the user first, or never runs in read-only mode.
 */
export class AiChat {
  entries: ChatEntry[] = []
  /** The last result with columns that the AI's SQL gave. */
  result: ResultSet | null = null
  /** What runs now. '' = nothing. */
  phase: '' | 'thinking' | 'running' = ''
  /** True when the last question ran SQL that can change data. */
  wrote = false
  /** The id of this chat in the saved history. */
  chatId: string = crypto.randomUUID()
  /** The AI tool of this chat. A chat goes on with its own tool. */
  provider = AI_DEFAULT_PROVIDER
  private created = 0
  private sessionId: string | null = null
  private options: AiOptions | null = null
  private stopped = false
  private api: DatabaseApi
  private prompt: DiscardPrompt

  constructor(api: DatabaseApi, prompt: DiscardPrompt) {
    this.api = api
    this.prompt = prompt
  }

  /** Asks the AI about the database of connection `id`, and runs its SQL until it is done. */
  async ask(id: string, question: string, options: AiOptions): Promise<void> {
    if (this.entries.length === 0) {
      this.provider = options.provider
      this.created = Date.now()
    }
    this.options = { ...options, provider: this.provider }
    this.stopped = false
    this.wrote = false
    this.entries.push({ kind: 'user', text: question })
    try {
      let reply = await this.think(id, question)
      for (let step = 0; reply.sql && !this.stopped; step += 1) {
        if (step >= AI_MAX_STEPS) {
          this.note(`Stopped after ${AI_MAX_STEPS} queries. Ask again to go on.`)
          return
        }
        const feedback = await this.runSql(id, reply.sql, reply.isRead)
        if (this.stopped) {
          break
        }
        reply = await this.think(id, feedback)
      }
    } catch (error) {
      if (!this.stopped) {
        // Shown in the chat too, like a model id the AI tool does not know.
        this.entries.push({ kind: 'note', text: String(error), failed: true })
        throw error
      }
    } finally {
      this.phase = ''
    }
    if (this.stopped) {
      this.note('Stopped')
    }
  }

  /** Stops the AI, and the SQL it runs now. The chat stays. */
  async stop(id: string): Promise<void> {
    if (this.phase === '') {
      return
    }
    this.stopped = true
    if (this.phase === 'running') {
      await this.api.cancelQuery(id)
    } else {
      await this.api.stopClaude(id)
    }
  }

  /** Starts a new conversation. The AI forgets the old one. */
  clear(): void {
    if (this.phase !== '') {
      return
    }
    this.entries = []
    this.result = null
    this.sessionId = null
    this.chatId = crypto.randomUUID()
    this.created = 0
  }

  /** The chat as it is saved. The result rows are left out: they can hold private data. */
  snapshot(connection: string, database: string): SavedChat | null {
    const first = this.entries[0]
    if (!first) {
      return null
    }
    const title = first.text.replace(/\s+/g, ' ').trim()
    return {
      id: this.chatId,
      title: title.length > AI_TITLE_MAX ? `${title.slice(0, AI_TITLE_MAX)}…` : title,
      provider: this.provider,
      connection,
      database,
      sessionId: this.sessionId,
      created: this.created,
      updated: Date.now(),
      // A plain copy, so later changes to this chat do not change the saved one.
      entries: JSON.parse(JSON.stringify(this.entries)) as ChatEntry[],
    }
  }

  /** Shows a saved chat. The next question goes on in its AI session. */
  restore(saved: SavedChat): void {
    if (this.phase !== '') {
      return
    }
    this.entries = JSON.parse(JSON.stringify(saved.entries)) as ChatEntry[]
    this.result = null
    this.chatId = saved.id
    this.provider = saved.provider
    this.created = saved.created
    this.sessionId = saved.sessionId
  }

  /** The chat as plain text, for a new AI session that lost the old one. */
  static transcript(entries: ChatEntry[]): string {
    const lines: string[] = []
    for (const entry of entries) {
      if (entry.kind === 'user') {
        lines.push(`User: ${entry.text}`)
      } else if (entry.kind === 'ai') {
        lines.push(`AI: ${entry.text}`)
      } else if (entry.kind === 'sql') {
        lines.push(`SQL (${entry.outcome ?? 'not run'}): ${entry.text}`)
      }
    }
    return lines.join('\n')
  }

  /** The text the AI gets for a result: the columns and the first rows. */
  static describe(result: ResultSet | null, affected: number): string {
    if (!result) {
      return `OK. ${affected} rows affected.`
    }
    const shown = result.rows.slice(0, AI_RESULT_ROWS)
    const total = result.truncated ? `more than ${result.rows.length}` : String(result.rows.length)
    const lines = [`Columns: ${result.columns.join(' | ')}`]
    lines.push(
      shown.length < result.rows.length
        ? `Rows: ${total}. The first ${shown.length} are below.`
        : `Rows: ${total}.`,
    )
    for (const row of shown) {
      const cells: string[] = []
      for (const cell of row) {
        cells.push(AiChat.cell(cell))
      }
      lines.push(cells.join(' | '))
    }
    if (affected > 0) {
      lines.push(`Affected rows: ${affected}`)
    }
    return lines.join('\n')
  }

  private static cell(value: string | null): string {
    if (value === null) {
      return 'NULL'
    }
    const flat = value.replace(/\s+/g, ' ')
    return flat.length > AI_CELL_MAX ? `${flat.slice(0, AI_CELL_MAX)}…` : flat
  }

  /** Sends one message to the AI and shows its reply text. */
  private async think(id: string, message: string): Promise<ClaudeReply> {
    this.phase = 'thinking'
    const options = this.options ?? {
      provider: this.provider,
      model: null,
      effort: null,
      readOnly: false,
    }
    let reply: ClaudeReply
    try {
      reply = await this.api.askClaude(id, message, this.sessionId, options)
    } catch (error) {
      if (!this.sessionId || !String(error).includes(AI_SESSION_GONE)) {
        throw error
      }
      // The AI tool has deleted the old session. Start a new one that gets the old messages as text.
      const earlier = AiChat.transcript(this.entries.slice(0, -1))
      this.note('The old AI session is gone. A new one starts with the old messages.')
      this.sessionId = null
      const text = `Earlier messages of this chat:\n${earlier}\n\nNew message: ${message}`
      reply = await this.api.askClaude(id, text, null, options)
    }
    this.sessionId = reply.sessionId
    if (reply.message) {
      this.entries.push({ kind: 'ai', text: reply.message })
    }
    return reply
  }

  /** Runs SQL from the AI. A write asks first. Gives the text that goes back to the AI. */
  private async runSql(id: string, sql: string, isRead: boolean): Promise<string> {
    this.entries.push({ kind: 'sql', text: sql })
    // Read it back from the list, so changes to it are seen by Vue.
    const entry = this.entries[this.entries.length - 1]
    if (!isRead && this.options?.readOnly) {
      entry.outcome = 'Not run: read-only mode'
      entry.failed = true
      return AI_READ_ONLY_REJECTED
    }
    if (!isRead && !(await this.prompt.ask(`The AI wants to run:\n\n${sql}`, AI_CONFIRM_TITLE))) {
      entry.outcome = 'Rejected'
      entry.failed = true
      return AI_REJECTED
    }
    this.wrote ||= !isRead
    this.phase = 'running'
    try {
      const sets = await this.api.runQuery(id, sql)
      let result: ResultSet | null = null
      let affected = 0
      for (const set of sets) {
        if (set.columns.length > 0) {
          result = set
        } else {
          affected += set.affected
        }
      }
      if (result) {
        // markRaw: Vue does not wrap the rows. A result is only replaced.
        this.result = markRaw(result)
        entry.outcome = `${result.rows.length}${result.truncated ? '+' : ''} rows`
      } else {
        entry.outcome = `${affected} rows affected`
      }
      return AiChat.describe(result, affected)
    } catch (error) {
      entry.outcome = String(error)
      entry.failed = true
      return `The query failed: ${String(error)}`
    }
  }

  private note(text: string): void {
    this.entries.push({ kind: 'note', text })
  }
}
