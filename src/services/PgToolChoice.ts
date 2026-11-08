import { PG_TOOL_VERSIONS } from '../constants/pgTools'
import type { PgTool, PgTools } from '../types'

/** One line of the version select. `tool` is null when this version is not installed. */
export interface PgToolOption {
  version: number
  tool: PgTool | null
}

/** The pick of a pg_dump or pg_restore version in the backup and restore dialog. */
export class PgToolChoice {
  /** 'pg_dump' or 'pg_restore'. */
  readonly name: string
  readonly server: number
  /** The listed versions plus any other installed one, newest first. */
  readonly options: PgToolOption[] = []
  /** The chosen major version. */
  version: number

  constructor(name: string, list: PgTools) {
    this.name = name
    this.server = list.server
    const versions = new Set<number>(PG_TOOL_VERSIONS)
    for (const tool of list.tools) {
      versions.add(tool.version)
    }
    for (const version of [...versions].sort(PgToolChoice.newestFirst)) {
      this.options.push({ version, tool: PgToolChoice.toolOf(list.tools, version) })
    }
    this.version = list.defaultVersion ?? this.fallbackVersion()
  }

  private static newestFirst(a: number, b: number): number {
    return b - a
  }

  private static toolOf(tools: PgTool[], version: number): PgTool | null {
    for (const tool of tools) {
      if (tool.version === version) {
        return tool
      }
    }
    return null
  }

  /** With no tool installed: the server version when it is listed, else the newest. */
  private fallbackVersion(): number {
    return PG_TOOL_VERSIONS.includes(this.server) ? this.server : PG_TOOL_VERSIONS[0]!
  }

  /** The text of one option, like `PostgreSQL 17.6` or `PostgreSQL 16 (not installed)`. */
  label(option: PgToolOption): string {
    if (option.tool === null) {
      return `PostgreSQL ${option.version} (not installed)`
    }
    return `PostgreSQL ${option.tool.release || option.version}`
  }

  /** Chooses `version` when it is listed. */
  pick(version: number): void {
    for (const option of this.options) {
      if (option.version === version) {
        this.version = version
      }
    }
  }

  /** True when the chosen version is installed. */
  get installed(): boolean {
    for (const option of this.options) {
      if (option.version === this.version) {
        return option.tool !== null
      }
    }
    return false
  }

  /** What to tell when no tool at all was found. '' when there is one. */
  get missing(): string {
    for (const option of this.options) {
      if (option.tool !== null) {
        return ''
      }
    }
    return `No ${this.name} was found. Install one as shown below.`
  }

  /** A warning when the chosen tool is older than the server. '' when it is fine or missing. */
  get warning(): string {
    if (!this.installed || this.version >= this.server) {
      return ''
    }
    const what =
      this.name === 'pg_dump'
        ? 'pg_dump does not back up a newer server.'
        : 'It may not read a file made by a newer pg_dump.'
    return `${this.name} ${this.version} is older than the server (PostgreSQL ${this.server}). ${what}`
  }
}

/** Something that lists the pg_dump or pg_restore versions, like a Session. */
export interface PgToolSource {
  pgTools(restore: boolean): Promise<PgTools>
}

/**
 * Loads the tool list for the dialog. Only the answer of the last load counts. A slow answer
 * from an older open of the dialog is dropped.
 */
export class PgToolLoader {
  private ticket = 0

  /** The new choice, or null when a newer load or `drop` came after it. Throws the error of the last load. */
  async load(source: PgToolSource, restoring: boolean): Promise<PgToolChoice | null> {
    this.ticket += 1
    const mine = this.ticket
    try {
      const list = await source.pgTools(restoring)
      if (mine !== this.ticket) {
        return null
      }
      return new PgToolChoice(restoring ? 'pg_restore' : 'pg_dump', list)
    } catch (error) {
      if (mine !== this.ticket) {
        return null
      }
      throw error
    }
  }

  /** Drops the answer of the load that runs now, for example when the dialog closes. */
  drop(): void {
    this.ticket += 1
  }
}
