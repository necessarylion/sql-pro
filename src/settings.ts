import { createApp, reactive } from 'vue'
import SettingsPage from './components/SettingsPage.vue'
import { LIGHT_SCHEME_QUERY } from './constants/theme'
import { DatabaseApi } from './services/DatabaseApi'
import { FontService } from './services/FontService'
import { FONT_SERVICE, SETTINGS_WINDOW, THEME_SERVICE } from './services/keys'
import { ProfileStorage } from './services/ProfileStorage'
import { SettingsWindow } from './services/SettingsWindow'
import { ThemeService } from './services/ThemeService'
import { WindowTheme } from './services/WindowTheme'
import '@fontsource-variable/inter'
import '@fontsource-variable/jetbrains-mono'
import './style.css'

// The Settings window. It builds only the objects its page needs, the same way as main.ts.
const api = new DatabaseApi()
// The same keys as the main window. A debug app gets its own keys here too.
const storage = new ProfileStorage(localStorage, ProfileStorage.current())
const themeService = reactive(
  new ThemeService(storage, new WindowTheme(), matchMedia(LIGHT_SCHEME_QUERY)),
) as ThemeService
const fontService = reactive(new FontService(api, storage, document.documentElement)) as FontService
const settingsWindow = new SettingsWindow(api, themeService, fontService)

// Set the theme and the fonts before the first paint.
themeService.start()
fontService.apply()

createApp(SettingsPage)
  .provide(THEME_SERVICE, themeService)
  .provide(FONT_SERVICE, fontService)
  .provide(SETTINGS_WINDOW, settingsWindow)
  .mount('#app')
