import { createApp, reactive } from 'vue'
import App from './App.vue'
import { ConnectionTransfer } from './services/ConnectionTransfer'
import { DatabaseApi } from './services/DatabaseApi'
import { DiagramLayoutStore } from './services/DiagramLayoutStore'
import { FavoriteStore } from './services/FavoriteStore'
import { LIGHT_SCHEME_QUERY } from './constants/theme'
import { DiscardPrompt } from './services/DiscardPrompt'
import {
  CONNECTION_TRANSFER,
  DIAGRAM_LAYOUTS,
  FAVORITE_STORE,
  LAYOUT_SERVICE,
  QUERY_HISTORY,
  SESSION_MANAGER,
  SHORTCUTS,
  THEME_SERVICE,
  ZOOM_SERVICE,
} from './services/keys'
import { LayoutService } from './services/LayoutService'
import { ProfileStorage } from './services/ProfileStorage'
import { QueryHistory } from './services/QueryHistory'
import { SessionManager } from './services/SessionManager'
import { Shortcuts } from './services/Shortcuts'
import { ThemeService } from './services/ThemeService'
import { WebviewZoom } from './services/WebviewZoom'
import { WindowTheme } from './services/WindowTheme'
import { ZoomService } from './services/ZoomService'
import '@fontsource-variable/inter'
import '@fontsource-variable/jetbrains-mono'
import './style.css'

// Build the object graph here. Components get the objects with inject.
// The casts are needed because `reactive` removes private members from the type.
const api = new DatabaseApi()
// The only place that touches localStorage. A debug app gets its own keys.
const storage = new ProfileStorage(localStorage, ProfileStorage.current())
const sessionManager = reactive(new SessionManager(api, new DiscardPrompt())) as SessionManager
const favoriteStore = reactive(new FavoriteStore(api, storage)) as FavoriteStore
const queryHistory = reactive(new QueryHistory(storage)) as QueryHistory
const themeService = reactive(
  new ThemeService(storage, new WindowTheme(), matchMedia(LIGHT_SCHEME_QUERY)),
) as ThemeService
// It puts the saved sidebar width on the page at once.
const layoutService = reactive(
  new LayoutService(storage, document.documentElement),
) as LayoutService

const zoomService = new ZoomService(storage, new WebviewZoom(), document.documentElement)

// Set the theme and the saved zoom before the first paint of the app.
themeService.start()
void zoomService.apply()

createApp(App)
  .provide(SESSION_MANAGER, sessionManager)
  .provide(FAVORITE_STORE, favoriteStore)
  .provide(CONNECTION_TRANSFER, new ConnectionTransfer(api, favoriteStore))
  .provide(QUERY_HISTORY, queryHistory)
  .provide(DIAGRAM_LAYOUTS, new DiagramLayoutStore(storage))
  .provide(THEME_SERVICE, themeService)
  .provide(LAYOUT_SERVICE, layoutService)
  .provide(ZOOM_SERVICE, zoomService)
  .provide(SHORTCUTS, new Shortcuts(navigator.platform))
  .mount('#app')
