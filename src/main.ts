import { createApp, reactive } from 'vue'
import App from './App.vue'
import { DatabaseApi } from './services/DatabaseApi'
import { FavoriteStore } from './services/FavoriteStore'
import { LIGHT_SCHEME_QUERY } from './constants/theme'
import { FAVORITE_STORE, SESSION_MANAGER, THEME_SERVICE } from './services/keys'
import { SessionManager } from './services/SessionManager'
import { ThemeService } from './services/ThemeService'
import { WindowTheme } from './services/WindowTheme'
import '@fontsource-variable/inter'
import '@fontsource-variable/jetbrains-mono'
import './style.css'

// Build the object graph here. Components get the objects with inject.
// The casts are needed because `reactive` removes private members from the type.
const api = new DatabaseApi()
const sessionManager = reactive(new SessionManager(api)) as SessionManager
const favoriteStore = reactive(new FavoriteStore(api)) as FavoriteStore
const themeService = reactive(
  new ThemeService(localStorage, new WindowTheme(), matchMedia(LIGHT_SCHEME_QUERY)),
) as ThemeService

// Set the theme before the first paint of the app.
themeService.start()

createApp(App)
  .provide(SESSION_MANAGER, sessionManager)
  .provide(FAVORITE_STORE, favoriteStore)
  .provide(THEME_SERVICE, themeService)
  .mount('#app')
