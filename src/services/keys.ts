import { inject, type InjectionKey } from 'vue'
import type { FavoriteStore } from './FavoriteStore'
import type { SessionManager } from './SessionManager'
import type { ThemeService } from './ThemeService'

// The keys for provide/inject. The objects are built in main.ts.
export const SESSION_MANAGER: InjectionKey<SessionManager> = Symbol('SessionManager')
export const FAVORITE_STORE: InjectionKey<FavoriteStore> = Symbol('FavoriteStore')
export const THEME_SERVICE: InjectionKey<ThemeService> = Symbol('ThemeService')

/** Like `inject`, but fails loudly when main.ts did not provide the object. */
export function injectRequired<T>(key: InjectionKey<T>): T {
  const value = inject(key)
  if (value === undefined) {
    throw new Error(`Nothing was provided for ${String(key.description)}`)
  }
  return value
}
