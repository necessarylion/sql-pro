import { DEFAULT_CONFIGS } from '../constants'
import type { ConnectionConfig, Driver } from '../types'

/** Changes the database type of a connection and keeps what the user filled in. */
export class DriverChange {
  /**
   * Between MySQL and PostgreSQL the host, password, database and SSH settings stay.
   * The port and the user change only when they were the defaults of the old type.
   */
  static apply(config: ConnectionConfig, driver: Driver): ConnectionConfig {
    if (config.driver === driver) {
      return config
    }
    const before = DEFAULT_CONFIGS[config.driver]
    const after = DEFAULT_CONFIGS[driver]
    // SQLite has no host, user or SSH, and its `database` is a file path.
    // Nothing can be carried over, so the new type starts from its defaults.
    if (config.driver === 'sqlite' || driver === 'sqlite') {
      return { ...after }
    }
    return {
      ...config,
      driver,
      // The form can hold the port as text while the user types.
      port: Number(config.port) === before.port ? after.port : config.port,
      user: config.user === before.user ? after.user : config.user,
    }
  }
}
