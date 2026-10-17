// One name, maybe quoted with `x`, "x" or [x].
export const SQL_NAME_PART = '(?:`[^`]+`|"[^"]+"|\\[[^\\]]+\\]|\\w+)'
// A table name, maybe with a schema in front.
const NAME = `${SQL_NAME_PART}(?:\\s*\\.\\s*${SQL_NAME_PART})?`
// A table name after FROM, JOIN, UPDATE or INTO.
const AFTER_KEYWORD = new RegExp(`\\b(?:from|join|update|into)\\s+(${NAME})`, 'gi')
// The alias right after a table name: `users u` or `users as u`.
const ALIAS = new RegExp(`^\\s+(?:as\\s+)?(${SQL_NAME_PART})`, 'i')
// One more table in a list: `from a x, b y`.
const NEXT_IN_LIST = new RegExp(`^\\s*,\\s*(${NAME})`, 'i')
// Words that can follow a table name. They are never an alias.
const NOT_ALIAS = new Set(
  (
    'where on using join inner left right full outer cross natural straight_join set values ' +
    'select group order limit offset fetch having union except intersect window for returning'
  ).split(' '),
)

/** One table in a query, with its alias when it has one. */
export interface SqlTableUse {
  name: string
  alias: string | null
}

/** Finds the tables a query reads from. It is a simple text scan, not a parser. */
export class SqlTableNames {
  /** Each name as `table` or `schema.table`, without quotes. Each name comes once. */
  static find(sql: string): string[] {
    const names = new Set<string>()
    for (const use of this.uses(sql)) {
      names.add(use.name)
    }
    return [...names]
  }

  /** The table that `qualifier` (an alias or a table name) means. Case does not matter. */
  static resolve(sql: string, qualifier: string): string {
    const wanted = this.clean(qualifier).toLowerCase()
    for (const use of this.uses(sql)) {
      if (use.alias?.toLowerCase() === wanted) {
        return use.name
      }
    }
    return this.clean(qualifier)
  }

  /** Removes quotes and spaces from a name. */
  static clean(name: string): string {
    return name.replace(/[`"[\]\s]/g, '')
  }

  private static uses(sql: string): SqlTableUse[] {
    const uses: SqlTableUse[] = []
    for (const match of sql.matchAll(AFTER_KEYWORD)) {
      let name = match[1]
      let rest = sql.slice(match.index + match[0].length)
      for (;;) {
        const alias = ALIAS.exec(rest)
        const aliasName = alias ? this.clean(alias[1]) : null
        if (alias && aliasName && !NOT_ALIAS.has(aliasName.toLowerCase())) {
          uses.push({ name: this.clean(name), alias: aliasName })
          rest = rest.slice(alias[0].length)
        } else {
          uses.push({ name: this.clean(name), alias: null })
        }
        const next = NEXT_IN_LIST.exec(rest)
        if (!next) {
          break
        }
        name = next[1]
        rest = rest.slice(next[0].length)
      }
    }
    return uses
  }
}
