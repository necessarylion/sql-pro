import { DRIVER_LABELS } from '../constants'
import {
  CONNECTIONS_FILE_ERROR,
  CONNECTIONS_FILE_NAME,
  CONNECTIONS_FILE_NEWER,
  CONNECTIONS_FILE_VERSION,
} from '../constants/transfer'
import type { ConnectionConfig, Driver, SshConfig } from '../types'
import type { DatabaseApi } from './DatabaseApi'
import { Environment } from './Environment'
import type { Favorite, FavoriteStore } from './FavoriteStore'

/** The JSON in an exported file. It never holds a password. */
interface ConnectionsFile {
  version: number
  connections: Favorite[]
}

/**
 * Exports the saved connections to a JSON file and imports them back.
 * Passwords stay in the keychain. They never go into the file.
 * Folders are not exported. Imported connections land at the top level.
 */
export class ConnectionTransfer {
  private api: DatabaseApi
  private store: FavoriteStore

  constructor(api: DatabaseApi, store: FavoriteStore) {
    this.api = api
    this.store = store
  }

  /**
   * Writes connections to the file at `path`. Returns how many.
   * With no `favorites` it writes all saved connections.
   */
  async exportTo(path: string, favorites: Favorite[] = this.store.favorites): Promise<number> {
    const connections: Favorite[] = []
    for (const favorite of favorites) {
      connections.push(ConnectionTransfer.clean(favorite))
    }
    const file: ConnectionsFile = { version: CONNECTIONS_FILE_VERSION, connections }
    await this.api.writeTextFile(path, JSON.stringify(file, null, 2))
    return connections.length
  }

  /**
   * Adds the connections of the file at `path` to the list. Returns how many.
   * A connection whose id is taken gets a new id, so no saved one is replaced.
   * Throws a plain message when the file is not a valid connections file.
   */
  async importFrom(path: string): Promise<number> {
    const connections = ConnectionTransfer.parse(await this.api.readTextFile(path))
    const taken = new Set<string>()
    for (const favorite of this.store.favorites) {
      taken.add(favorite.id)
    }
    for (const favorite of connections) {
      if (taken.has(favorite.id)) {
        favorite.id = crypto.randomUUID()
      }
      taken.add(favorite.id)
    }
    this.store.addAll(connections)
    return connections.length
  }

  /** A safe file name for the export of one connection, like `shop-db.json`. */
  static fileName(name: string): string {
    const base = name
      .trim()
      .replace(/[^A-Za-z0-9._-]+/g, '-')
      .replace(/^[-.]+|-+$/g, '')
    return base ? `${base}.json` : CONNECTIONS_FILE_NAME
  }

  /** The connections of a file. Throws when the text is not a valid file of a known version. */
  static parse(text: string): Favorite[] {
    let data: unknown
    try {
      data = JSON.parse(text)
    } catch {
      throw new Error(CONNECTIONS_FILE_ERROR)
    }
    if (!ConnectionTransfer.isObject(data) || typeof data.version !== 'number') {
      throw new Error(CONNECTIONS_FILE_ERROR)
    }
    if (data.version > CONNECTIONS_FILE_VERSION) {
      throw new Error(CONNECTIONS_FILE_NEWER)
    }
    if (data.version < 1 || !Array.isArray(data.connections)) {
      throw new Error(CONNECTIONS_FILE_ERROR)
    }
    const connections: Favorite[] = []
    for (const item of data.connections) {
      connections.push(ConnectionTransfer.read(item))
    }
    return connections
  }

  /** A copy with only the known fields and with no passwords. */
  private static clean(favorite: Favorite): Favorite {
    const config = favorite.config
    const ssh = config.ssh
    return {
      id: favorite.id,
      name: favorite.name,
      environment: favorite.environment,
      config: {
        driver: config.driver,
        host: config.host,
        port: config.port,
        user: config.user,
        password: '',
        database: config.database,
        ssh: ssh
          ? { host: ssh.host, port: ssh.port, user: ssh.user, password: '', keyPath: ssh.keyPath }
          : null,
      },
    }
  }

  /** One connection of a file. Throws when a field is missing or has the wrong type. */
  private static read(item: unknown): Favorite {
    if (!ConnectionTransfer.isObject(item) || !ConnectionTransfer.isObject(item.config)) {
      throw new Error(CONNECTIONS_FILE_ERROR)
    }
    const config = item.config
    const driver = config.driver
    if (
      typeof item.id !== 'string' ||
      !item.id ||
      typeof item.name !== 'string' ||
      typeof driver !== 'string' ||
      !Object.hasOwn(DRIVER_LABELS, driver) ||
      !ConnectionTransfer.hasServer(config) ||
      typeof config.database !== 'string'
    ) {
      throw new Error(CONNECTIONS_FILE_ERROR)
    }
    const result: ConnectionConfig = {
      driver: driver as Driver,
      host: config.host as string,
      port: config.port as number,
      user: config.user as string,
      password: '',
      database: config.database,
      ssh: ConnectionTransfer.readSsh(config.ssh),
    }
    return {
      id: item.id,
      name: item.name,
      environment: Environment.orLocal(item.environment),
      config: result,
    }
  }

  private static readSsh(ssh: unknown): SshConfig | null {
    if (ssh === null || ssh === undefined) {
      return null
    }
    if (
      !ConnectionTransfer.isObject(ssh) ||
      !ConnectionTransfer.hasServer(ssh) ||
      typeof ssh.keyPath !== 'string'
    ) {
      throw new Error(CONNECTIONS_FILE_ERROR)
    }
    return {
      host: ssh.host as string,
      port: ssh.port as number,
      user: ssh.user as string,
      password: '',
      keyPath: ssh.keyPath,
    }
  }

  /** True when `value` has a text `host`, a number `port` and a text `user`. */
  private static hasServer(value: Record<string, unknown>): boolean {
    return (
      typeof value.host === 'string' &&
      typeof value.port === 'number' &&
      Number.isInteger(value.port) &&
      typeof value.user === 'string'
    )
  }

  private static isObject(value: unknown): value is Record<string, unknown> {
    return typeof value === 'object' && value !== null && !Array.isArray(value)
  }
}
