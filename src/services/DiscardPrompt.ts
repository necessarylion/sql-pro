import { confirm } from '@tauri-apps/plugin-dialog'
import { UNSAVED_TITLE } from '../constants'

/**
 * Asks the user before unsaved changes are lost, and before a save deletes rows.
 * Tests give the Session a fake one.
 */
export class DiscardPrompt {
  ask(message: string, title = UNSAVED_TITLE): Promise<boolean> {
    return confirm(message, { title, kind: 'warning' })
  }
}
