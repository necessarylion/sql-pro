import { FAVORITE_COPY_SUFFIX, FAVORITES_STORAGE_KEY, SSH_PASSWORD_SUFFIX } from '../constants'
import {
  DEFAULT_FOLDER_COLOR,
  DEFAULT_FOLDER_ICON,
  FOLDER_COLORS,
  FOLDER_ICONS,
  FOLDER_NAME_MAX_LENGTH,
  FOLDERS_STORAGE_KEY,
} from '../constants/folders'
import type { FolderColor, FolderIconName } from '../constants/folders'
import type { ConnectionConfig } from '../types'
import type { DatabaseApi } from './DatabaseApi'
import { Environment } from './Environment'
import type { ProfileStorage } from './ProfileStorage'

export interface Favorite {
  id: string
  name: string
  /** A lower-case name like `local` or `prod`. '' = none. */
  environment: string
  /** The id of its folder. '' or missing = no folder (top level). */
  folderId?: string
  config: ConnectionConfig
}

export interface Folder {
  id: string
  name: string
  color: FolderColor
  icon: FolderIconName
  /** True when the folder is closed in the list. */
  collapsed: boolean
}

/** One part of the list: a folder with its connections, or the top level. */
export interface FavoriteSection {
  /** null = the connections with no folder. */
  folder: Folder | null
  favorites: Favorite[]
  /** False when the connections are hidden. */
  open: boolean
}

export interface LoadedConfig {
  config: ConnectionConfig
  /** The keychain error when a password could not be read. '' = all fine. */
  keychainError: string
}

export interface DuplicatedFavorite {
  /** The new copy, as it is in the list. */
  favorite: Favorite
  /** The keychain error when a password could not be copied. '' = all fine. */
  keychainError: string
}

/**
 * Saved connections and their folders.
 * A connection is in one folder or in none. A folder holds no other folder.
 * The list is kept in the storage of the profile, without the password.
 * The passwords (database and SSH) are kept in the OS keychain.
 */
export class FavoriteStore {
  favorites: Favorite[]
  folders: Folder[]
  private api: DatabaseApi
  private storage: ProfileStorage

  constructor(api: DatabaseApi, storage: ProfileStorage) {
    this.api = api
    this.storage = storage
    this.favorites = this.read()
    this.folders = this.readFolders()
  }

  /**
   * Adds the favorite, or replaces the one with the same id.
   * The favorite is always saved. Returns the keychain error when a password
   * could not be stored. Returns '' when all is fine.
   */
  async save(favorite: Favorite): Promise<string> {
    const config = favorite.config
    const ssh = config.ssh ? { ...config.ssh, password: '' } : null
    const stored: Favorite = {
      ...favorite,
      environment: Environment.orLocal(favorite.environment),
      folderId: this.folderOf(favorite),
      config: { ...config, password: '', ssh },
    }
    const index = this.indexOf(favorite.id)
    if (index < 0) {
      this.favorites.push(stored)
    } else {
      this.favorites[index] = stored
    }
    this.write()
    const first = await this.keep(favorite.id, config.password)
    const second = await this.keep(favorite.id + SSH_PASSWORD_SUFFIX, config.ssh?.password ?? '')
    return first || second
  }

  /**
   * Removes the favorite from the list. This always works.
   * Returns the keychain error when a password entry could not be removed.
   * A password entry that stays behind does no harm.
   */
  async remove(id: string): Promise<string> {
    const index = this.indexOf(id)
    if (index >= 0) {
      this.favorites.splice(index, 1)
      this.write()
    }
    const first = await this.keep(id, '')
    const second = await this.keep(id + SSH_PASSWORD_SUFFIX, '')
    return first || second
  }

  /**
   * The config of a favorite, with its passwords read back from the keychain.
   * A password the keychain does not give out is left empty.
   * `keychainError` is then the error text. It is '' when all is fine.
   */
  async fullConfig(favorite: Favorite): Promise<LoadedConfig> {
    const errors: string[] = []
    const password = await this.secret(favorite.id, errors)
    // Favorites saved before SSH support have no `ssh` field.
    const saved = favorite.config.ssh ?? null
    const ssh = saved
      ? { ...saved, password: await this.secret(favorite.id + SSH_PASSWORD_SUFFIX, errors) }
      : null
    return { config: { ...favorite.config, password, ssh }, keychainError: errors[0] ?? '' }
  }

  /**
   * Makes a copy of the favorite, right after it in the list.
   * The copy has a new id and the name `<name> copy`. The passwords are copied too.
   * The copy is always made. When the keychain fails, the copy has no password
   * and `keychainError` is the error text.
   */
  async duplicate(favorite: Favorite): Promise<DuplicatedFavorite> {
    const loaded = await this.fullConfig(favorite)
    const copy: Favorite = {
      ...favorite,
      id: crypto.randomUUID(),
      name: this.copyName(favorite.name),
    }
    // The copy takes its place first. `save` then finds it by id and fills it in.
    const index = this.indexOf(favorite.id)
    this.favorites.splice(index < 0 ? this.favorites.length : index + 1, 0, copy)
    const failure = await this.save({ ...copy, config: loaded.config })
    return {
      favorite: this.favorites[this.indexOf(copy.id)],
      keychainError: loaded.keychainError || failure,
    }
  }

  /**
   * Adds a folder, or changes the one with this id. '' as id = a new folder.
   * Returns false and does nothing when the name is empty.
   */
  saveFolder(id: string, name: string, color: string, icon: string): boolean {
    const clean = name.trim().slice(0, FOLDER_NAME_MAX_LENGTH)
    if (!clean) {
      return false
    }
    let folder = this.folder(id)
    if (!folder) {
      folder = {
        id: crypto.randomUUID(),
        name: '',
        color: DEFAULT_FOLDER_COLOR,
        icon: DEFAULT_FOLDER_ICON,
        collapsed: false,
      }
      this.folders.push(folder)
    }
    folder.name = clean
    folder.color = FavoriteStore.known(FOLDER_COLORS, color, DEFAULT_FOLDER_COLOR)
    folder.icon = FavoriteStore.known(FOLDER_ICONS, icon, DEFAULT_FOLDER_ICON)
    this.writeFolders()
    return true
  }

  /** Removes the folder. Its connections stay. They move back to the top level. */
  removeFolder(id: string): void {
    for (const favorite of this.inFolder(id)) {
      favorite.folderId = ''
    }
    const kept: Folder[] = []
    for (const folder of this.folders) {
      if (folder.id !== id) {
        kept.push(folder)
      }
    }
    this.folders = kept
    this.write()
    this.writeFolders()
  }

  /** Opens or closes the folder in the list. The state is saved. */
  toggleFolder(id: string): void {
    const folder = this.folder(id)
    if (folder) {
      folder.collapsed = !folder.collapsed
      this.writeFolders()
    }
  }

  /** Puts a connection into a folder. '' or an unknown folder id = the top level. */
  move(favoriteId: string, folderId: string): void {
    const favorite = this.favorites[this.indexOf(favoriteId)]
    if (favorite) {
      favorite.folderId = this.folder(folderId) ? folderId : ''
      this.write()
    }
  }

  /** The id of the folder of a connection. '' when it has none, or the folder is gone. */
  folderOf(favorite: Favorite): string {
    return this.folder(favorite.folderId ?? '')?.id ?? ''
  }

  /** The saved connections of one folder. '' = the top level. */
  inFolder(folderId: string): Favorite[] {
    const found: Favorite[] = []
    for (const favorite of this.favorites) {
      if (this.folderOf(favorite) === folderId) {
        found.push(favorite)
      }
    }
    return found
  }

  /**
   * The list to show: the folders by name, then the top level.
   * With a search text, only connections with a matching name are listed.
   * A folder with a matching name shows all its connections.
   * A folder with no match is left out. The folders that stay are shown open.
   */
  sections(search: string): FavoriteSection[] {
    const needle = search.trim().toLowerCase()
    const sections: FavoriteSection[] = []
    for (const folder of [...this.folders].sort(FavoriteStore.byName)) {
      const all = this.inFolder(folder.id)
      // With no search text every folder name matches.
      const whole = folder.name.toLowerCase().includes(needle)
      const favorites = whole ? all : FavoriteStore.matching(all, needle)
      if (whole || favorites.length > 0) {
        sections.push({ folder, favorites, open: needle ? true : !folder.collapsed })
      }
    }
    const top = FavoriteStore.matching(this.inFolder(''), needle)
    sections.push({ folder: null, favorites: top, open: true })
    return sections
  }

  private static byName(first: Folder, second: Folder): number {
    return first.name.localeCompare(second.name)
  }

  private static matching(favorites: Favorite[], needle: string): Favorite[] {
    const matches: Favorite[] = []
    for (const favorite of favorites) {
      if (favorite.name.toLowerCase().includes(needle)) {
        matches.push(favorite)
      }
    }
    return matches
  }

  /** `value` when it is one of `allowed`. Any other value gives `fallback`. */
  private static known<T extends string>(allowed: readonly T[], value: unknown, fallback: T): T {
    for (const name of allowed) {
      if (name === value) {
        return name
      }
    }
    return fallback
  }

  /** `<name> copy`. When that name is taken: `<name> copy 2`, `<name> copy 3`, ... */
  private copyName(name: string): string {
    const base = name + FAVORITE_COPY_SUFFIX
    let candidate = base
    for (let count = 2; this.hasName(candidate); count++) {
      candidate = `${base} ${count}`
    }
    return candidate
  }

  private hasName(name: string): boolean {
    for (const favorite of this.favorites) {
      if (favorite.name === name) {
        return true
      }
    }
    return false
  }

  private folder(id: string): Folder | undefined {
    for (const folder of this.folders) {
      if (folder.id === id) {
        return folder
      }
    }
    return undefined
  }

  /** Reads one password. On failure it adds the error to `errors` and returns ''. */
  private async secret(id: string, errors: string[]): Promise<string> {
    try {
      return await this.api.loadPassword(id)
    } catch (error) {
      errors.push(String(error))
      return ''
    }
  }

  /**
   * Saves a password, or removes the entry when the password is empty.
   * Never throws. Returns the error text, or '' when it worked.
   */
  private async keep(id: string, password: string): Promise<string> {
    try {
      if (password) {
        await this.api.savePassword(id, password)
      } else {
        await this.api.deletePassword(id)
      }
      return ''
    } catch (error) {
      return String(error)
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
      const saved: unknown = JSON.parse(this.storage.getItem(FAVORITES_STORAGE_KEY) ?? '[]')
      const favorites: Favorite[] = Array.isArray(saved) ? saved : []
      // Favorites saved before environments have no such field.
      for (const favorite of favorites) {
        favorite.environment = Environment.orLocal(favorite.environment)
      }
      return favorites
    } catch {
      // Broken saved data. Start with an empty list.
      return []
    }
  }

  /** Saved data from before folders has no folder list. That gives no folders. */
  private readFolders(): Folder[] {
    const folders: Folder[] = []
    try {
      const saved: unknown = JSON.parse(this.storage.getItem(FOLDERS_STORAGE_KEY) ?? '[]')
      for (const item of Array.isArray(saved) ? saved : []) {
        if (item && typeof item.id === 'string' && typeof item.name === 'string') {
          folders.push({
            id: item.id,
            name: item.name,
            color: FavoriteStore.known(FOLDER_COLORS, item.color, DEFAULT_FOLDER_COLOR),
            icon: FavoriteStore.known(FOLDER_ICONS, item.icon, DEFAULT_FOLDER_ICON),
            collapsed: item.collapsed === true,
          })
        }
      }
    } catch {
      // Broken saved data. Start with no folders.
    }
    return folders
  }

  private write(): void {
    this.storage.setItem(FAVORITES_STORAGE_KEY, JSON.stringify(this.favorites))
  }

  private writeFolders(): void {
    this.storage.setItem(FOLDERS_STORAGE_KEY, JSON.stringify(this.folders))
  }
}
