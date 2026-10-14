import { inject, type InjectionKey } from 'vue'
import type { DiagramLayoutStore } from './DiagramLayoutStore'
import type { FavoriteStore } from './FavoriteStore'
import type { LayoutService } from './LayoutService'
import type { QueryHistory } from './QueryHistory'
import type { SessionManager } from './SessionManager'
import type { Shortcuts } from './Shortcuts'
import type { ThemeService } from './ThemeService'
import type { ZoomService } from './ZoomService'

// The keys for provide/inject. The objects are built in main.ts.
export const SESSION_MANAGER: InjectionKey<SessionManager> = Symbol('SessionManager')
export const FAVORITE_STORE: InjectionKey<FavoriteStore> = Symbol('FavoriteStore')
export const THEME_SERVICE: InjectionKey<ThemeService> = Symbol('ThemeService')
export const LAYOUT_SERVICE: InjectionKey<LayoutService> = Symbol('LayoutService')
export const ZOOM_SERVICE: InjectionKey<ZoomService> = Symbol('ZoomService')
export const SHORTCUTS: InjectionKey<Shortcuts> = Symbol('Shortcuts')
export const QUERY_HISTORY: InjectionKey<QueryHistory> = Symbol('QueryHistory')
export const DIAGRAM_LAYOUTS: InjectionKey<DiagramLayoutStore> = Symbol('DiagramLayoutStore')

/** Like `inject`, but fails loudly when main.ts did not provide the object. */
export function injectRequired<T>(key: InjectionKey<T>): T {
  const value = inject(key)
  if (value === undefined) {
    throw new Error(`Nothing was provided for ${String(key.description)}`)
  }
  return value
}
