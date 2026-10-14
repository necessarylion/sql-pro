declare global {
  /**
   * Set by the Rust app before the scripts of the page run.
   * It is 'dev' in every debug build and '' in a release build.
   * It is missing in a plain browser and in tests.
   */
  var __SQL_PRO_PROFILE__: string | undefined
}

/**
 * The saved data of the app, kept apart for each profile.
 * A debug app and a release app share one localStorage. So the debug app
 * uses its own keys. It then never sees the connections of the release app.
 * All saved data must go through this class. Do not use localStorage directly.
 */
export class ProfileStorage {
  private storage: Storage
  private profile: string

  /** `profile` is '' for the release app. See `current`. */
  constructor(storage: Storage, profile: string) {
    this.storage = storage
    this.profile = profile
  }

  /** The profile of the running app. A missing value means release (''). */
  static current(): string {
    const profile: unknown = globalThis.__SQL_PRO_PROFILE__
    return typeof profile === 'string' ? profile : ''
  }

  /**
   * The real key for a base key. The release key is the base key itself.
   * In the dev profile `sql-pro.favorites` becomes `sql-pro.favorites.dev`.
   */
  key(base: string): string {
    return this.profile ? `${base}.${this.profile}` : base
  }

  getItem(base: string): string | null {
    return this.storage.getItem(this.key(base))
  }

  setItem(base: string, value: string): void {
    this.storage.setItem(this.key(base), value)
  }
}
