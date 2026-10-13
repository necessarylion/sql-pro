import { FAVORITES_STORAGE_KEY, SSH_PASSWORD_SUFFIX } from '../constants'
import type { ConnectionConfig } from '../types'
import type { DatabaseApi } from './DatabaseApi'

export interface Favorite {
  id: string
  name: string
  config: ConnectionConfig
}

/**
 * Saved connections.
 * The list is kept in localStorage without the password.
 * The passwords (database and SSH) are kept in the OS keychain.
 */
export class FavoriteStore {
  favorites: Favorite[]
  private api: DatabaseApi

  constructor(api: DatabaseApi) {
    this.api = api
    this.favorites = this.read()
  }

  /** Adds the favorite, or replaces the one with the same id. */
  async save(favorite: Favorite): Promise<void> {
    const config = favorite.config
    const ssh = config.ssh ? { ...config.ssh, password: '' } : null
    const stored: Favorite = { ...favorite, config: { ...config, password: '', ssh } }
    const index = this.indexOf(favorite.id)
    if (index < 0) {
      this.favorites.push(stored)
    } else {
      this.favorites[index] = stored
    }
    this.write()
    await this.keep(favorite.id, config.password)
    await this.keep(favorite.id + SSH_PASSWORD_SUFFIX, config.ssh?.password ?? '')
  }

  async remove(id: string): Promise<void> {
    const index = this.indexOf(id)
    if (index >= 0) {
      this.favorites.splice(index, 1)
      this.write()
    }
    await this.api.deletePassword(id)
    await this.api.deletePassword(id + SSH_PASSWORD_SUFFIX)
  }

  /** The config of a favorite, with its passwords read back from the keychain. */
  async fullConfig(favorite: Favorite): Promise<ConnectionConfig> {
    const password = await this.api.loadPassword(favorite.id)
    // Favorites saved before SSH support have no `ssh` field.
    const saved = favorite.config.ssh ?? null
    const ssh = saved
      ? { ...saved, password: await this.api.loadPassword(favorite.id + SSH_PASSWORD_SUFFIX) }
      : null
    return { ...favorite.config, password, ssh }
  }

  /** Saves a password, or removes the entry when the password is empty. */
  private async keep(id: string, password: string): Promise<void> {
    if (password) {
      await this.api.savePassword(id, password)
    } else {
      await this.api.deletePassword(id)
    }
  }

  private indexOf(id: string): number {
    for (const [index, favorite] of this.favorites.entries()) {
      if (favorite.id === id) {
        return index
      }
    }
    return -1
  }

  private read(): Favorite[] {
    try {
      const saved: unknown = JSON.parse(localStorage.getItem(FAVORITES_STORAGE_KEY) ?? '[]')
      return Array.isArray(saved) ? saved : []
    } catch {
      // Broken saved data. Start with an empty list.
      return []
    }
  }

  private write(): void {
    localStorage.setItem(FAVORITES_STORAGE_KEY, JSON.stringify(this.favorites))
  }
}
