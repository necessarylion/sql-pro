import type { PgTool, PgTools } from '../types'

/** The pick of a pg_dump or pg_restore version in the backup and restore dialog. */
export class PgToolChoice {
  /** 'pg_dump' or 'pg_restore'. */
  readonly name: string
  readonly server: number
  /** Newest first. */
  readonly tools: PgTool[]
  /** The chosen major version. null when no tool was found. */
  version: number | null

  constructor(name: string, list: PgTools) {
    this.name = name
    this.server = list.server
    this.tools = list.tools
    this.version = list.defaultVersion
  }

  /** The text of one option, like `PostgreSQL 17`. */
  label(tool: PgTool): string {
    return `PostgreSQL ${tool.version}`
  }

  /** What to tell when no tool was found. '' when there is one. */
  get missing(): string {
    if (this.tools.length > 0) {
      return ''
    }
    return `${this.name} was not found. Install the PostgreSQL client tools (macOS: brew install libpq).`
  }

  /** A warning when the chosen tool is older than the server. '' when it is fine. */
  get warning(): string {
    if (this.version === null || this.version >= this.server) {
      return ''
    }
    const what =
      this.name === 'pg_dump'
        ? 'pg_dump does not back up a newer server.'
        : 'It may not read a file made by a newer pg_dump.'
    return `${this.name} ${this.version} is older than the server (PostgreSQL ${this.server}). ${what}`
  }
}
