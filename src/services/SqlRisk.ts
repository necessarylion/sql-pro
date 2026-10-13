import {
  ALWAYS_RISKY_KEYWORDS,
  NEEDS_WHERE_KEYWORDS,
  RISK_LIST_LIMIT,
  RISK_PREVIEW_LENGTH,
} from '../constants/risk'
import type { Driver } from '../types'

/**
 * Finds SQL statements that can change or remove a lot of data.
 *
 * ponytail: this is a safety net, not a SQL parser. Known limits:
 * - Only the first word of a statement is checked. `WITH ... DELETE` and
 *   `EXPLAIN ANALYZE DELETE` are seen as safe.
 * - A WHERE inside a subquery counts as the WHERE of the statement.
 * - PostgreSQL `$$...$$` strings, `E'...'` strings and `DELIMITER` scripts are not known.
 *   A `;` or a quote inside them can give a wrong answer.
 * - MySQL modes that change quoting (NO_BACKSLASH_ESCAPES) are not known.
 * Use a real SQL parser if these cases start to matter.
 */
export class SqlRisk {
  /** MySQL has `#` comments, backslash escapes and `/*! ... *\/` code. The others do not. */
  private mysql: boolean

  constructor(driver?: Driver) {
    this.mysql = driver === 'mysql'
  }

  /** The text for the confirm dialog. Empty when the SQL is safe. */
  message(sql: string): string {
    const risks = this.find(sql)
    if (risks.length === 0) {
      return ''
    }
    const lines = risks.slice(0, RISK_LIST_LIMIT)
    if (risks.length > RISK_LIST_LIMIT) {
      lines.push(`and ${risks.length - RISK_LIST_LIMIT} more`)
    }
    return `${lines.join('\n')}\n\nThis can change or remove a lot of data.`
  }

  /** One line for each risky statement, like "DROP: DROP TABLE orders". */
  find(sql: string): string[] {
    const masked = this.mask(sql)
    const risks: string[] = []
    let start = 0
    while (start < masked.length) {
      let end = masked.indexOf(';', start)
      if (end < 0) {
        end = masked.length
      }
      const risk = this.judge(masked.slice(start, end), sql.slice(start, end))
      if (risk) {
        risks.push(risk)
      }
      start = end + 1
    }
    return risks
  }

  /** `code` is one statement with strings and comments masked. `raw` is the same part as typed. */
  private judge(code: string, raw: string): string | null {
    const first = /[A-Za-z]+/.exec(code)
    if (!first) {
      return null
    }
    const keyword = first[0].toUpperCase()
    let label: string
    if (ALWAYS_RISKY_KEYWORDS.includes(keyword)) {
      label = keyword
    } else if (NEEDS_WHERE_KEYWORDS.includes(keyword) && !/\bWHERE\b/i.test(code)) {
      label = `${keyword} without WHERE`
    } else {
      return null
    }
    const text = raw.slice(first.index).replace(/\s+/g, ' ').trim()
    const cut = text.length > RISK_PREVIEW_LENGTH
    return `${label}: ${cut ? `${text.slice(0, RISK_PREVIEW_LENGTH)}…` : text}`
  }

  /** The same text with the same length. Strings and comments are turned into spaces. */
  private mask(sql: string): string {
    let masked = ''
    let index = 0
    while (index < sql.length) {
      const end = this.hiddenEnd(sql, index)
      if (end > index) {
        masked += ' '.repeat(end - index)
        index = end
      } else {
        masked += sql[index]
        index += 1
      }
    }
    return masked
  }

  /** The end of the string or comment that starts at `start`. Returns `start` when none does. */
  private hiddenEnd(sql: string, start: number): number {
    const char = sql[start]
    if (char === "'" || char === '"' || char === '`') {
      return this.quoteEnd(sql, start)
    }
    if (sql.startsWith('/*', start)) {
      // MySQL runs the code inside `/*! ... */`. Hide only the opening, keep the code.
      if (this.mysql && sql[start + 2] === '!') {
        return start + 3
      }
      return this.endAfter(sql, '*/', start + 2)
    }
    if (this.lineCommentAt(sql, start)) {
      return this.endAfter(sql, '\n', start)
    }
    return start
  }

  /** In MySQL, `--` starts a comment only when a space follows. `1--1` is math there. */
  private lineCommentAt(sql: string, start: number): boolean {
    if (sql.startsWith('--', start)) {
      return !this.mysql || /^\s?$/.test(sql.charAt(start + 2))
    }
    return this.mysql && sql[start] === '#'
  }

  /** A doubled quote closes one string and opens the next. So it needs no special case. */
  private quoteEnd(sql: string, start: number): number {
    const quote = sql[start]
    let index = start + 1
    while (index < sql.length && sql[index] !== quote) {
      index += this.mysql && quote !== '`' && sql[index] === '\\' ? 2 : 1
    }
    return Math.min(index + 1, sql.length)
  }

  /** The position after the next `mark`. The end of the text when there is no `mark`. */
  private endAfter(sql: string, mark: string, from: number): number {
    const found = sql.indexOf(mark, from)
    return found < 0 ? sql.length : found + mark.length
  }
}
