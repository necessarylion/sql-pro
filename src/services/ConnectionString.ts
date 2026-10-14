import { CONNECTION_STRING_DRIVERS, CONNECTION_STRING_ERROR, DEFAULT_CONFIGS } from '../constants'
import type { ConnectionConfig } from '../types'

/** Reads a connection string like `postgres://user:password@host:5432/database`. */
export class ConnectionString {
  /**
   * Missing parts keep the defaults of the driver.
   * The error text never holds the input, because the input can hold a password.
   */
  static parse(text: string): ConnectionConfig {
    try {
      return ConnectionString.read(text.trim())
    } catch {
      throw new Error(CONNECTION_STRING_ERROR)
    }
  }

  /** Throws on bad input. The error of `URL` can hold the input, so `parse` replaces it. */
  private static read(text: string): ConnectionConfig {
    // `URL` has its own rules for `file:` (it makes every path absolute). `sqlite:` has none.
    const url = new URL(text.replace(/^file:/i, 'sqlite:'))
    const base = DEFAULT_CONFIGS[CONNECTION_STRING_DRIVERS[url.protocol.slice(0, -1)]]
    // ponytail: the query string and the hash are ignored. Options like `sslmode` have
    // no place in the app yet. Read `url.searchParams` here when they get one.
    if (base.driver === 'sqlite') {
      // `sqlite://relative.db` puts the first part of the path in the host.
      // `/C:/data/app.db` is a Windows path. It loses the first `/`.
      const path = decodeURIComponent(url.host + url.pathname).replace(/^\/([a-z]:)/i, '$1')
      if (!path) {
        throw new Error(CONNECTION_STRING_ERROR)
      }
      return { ...base, database: path }
    }
    return {
      ...base,
      // An IPv6 host comes as `[::1]`. The host field takes it without the brackets.
      host: url.hostname.replace(/^\[(.*)\]$/, '$1') || base.host,
      port: Number(url.port) || base.port,
      user: decodeURIComponent(url.username) || base.user,
      password: decodeURIComponent(url.password),
      database: decodeURIComponent(url.pathname.slice(1)) || base.database,
    }
  }
}
