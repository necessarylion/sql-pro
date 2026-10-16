import {
  AI_CHATS_STORAGE_KEY,
  AI_DEFAULT_PROVIDER,
  AI_MAX_CHATS,
  AI_MODEL_PATTERN,
  AI_PROVIDERS,
  AI_SETTINGS_STORAGE_KEY,
  type AiChoice,
  type AiProvider,
} from '../constants/ai'
import type { AiMode, AiOptions, ConnectionConfig, SavedChat } from '../types'
import type { ProfileStorage } from './ProfileStorage'

/** The last choices in Ask AI, and the saved chats. Both are kept through `ProfileStorage`. */
export class AiStore {
  provider = AI_DEFAULT_PROVIDER
  /** '' = the tool's own default. */
  model = ''
  effort = ''
  mode: AiMode = 'ask'
  /** Newest first. */
  chats: SavedChat[] = []
  private storage: ProfileStorage

  constructor(storage: ProfileStorage) {
    this.storage = storage
    const settings = this.read(AI_SETTINGS_STORAGE_KEY) as Partial<AiStore> | null
    if (settings) {
      this.provider = String(settings.provider ?? AI_DEFAULT_PROVIDER)
      this.model = String(settings.model ?? '')
      this.effort = String(settings.effort ?? '')
      this.mode = settings.mode === 'readOnly' ? 'readOnly' : 'ask'
    }
    this.fixChoices()
    const chats = this.read(AI_CHATS_STORAGE_KEY)
    this.chats = Array.isArray(chats) ? (chats as SavedChat[]) : []
  }

  /** Names a server, so a chat shows only with tabs of the same server. No password. */
  static connectionKey(config: ConnectionConfig | null): string {
    if (!config) {
      return ''
    }
    if (config.driver === 'sqlite') {
      return `sqlite:${config.database}`
    }
    return `${config.driver}:${config.user}@${config.host}:${config.port}`
  }

  /** The tool of the provider. An unknown id gives the default tool. */
  get current(): AiProvider {
    return AI_PROVIDERS[this.provider] ?? AI_PROVIDERS[AI_DEFAULT_PROVIDER]
  }

  /** What goes with the next message. `provider` is the provider of the chat. */
  options(provider: string): AiOptions {
    // A chat with another provider does not get this provider's model or effort.
    const own = provider === this.provider
    return {
      provider,
      model: own && this.model ? this.model : null,
      effort: own && this.effort ? this.effort : null,
      readOnly: this.mode === 'readOnly',
    }
  }

  /** Saves the choices. A model or effort the provider does not have goes back to default. */
  saveSettings(): void {
    this.fixChoices()
    const { provider, model, effort, mode } = this
    this.storage.setItem(AI_SETTINGS_STORAGE_KEY, JSON.stringify({ provider, model, effort, mode }))
  }

  /** The chats about one server, newest first. */
  chatsOf(connection: string): SavedChat[] {
    const found: SavedChat[] = []
    for (const chat of this.chats) {
      if (chat.connection === connection) {
        found.push(chat)
      }
    }
    return found
  }

  /** Adds or replaces a chat. Only the newest `AI_MAX_CHATS` are kept. */
  save(chat: SavedChat): void {
    const others: SavedChat[] = []
    for (const saved of this.chats) {
      if (saved.id !== chat.id) {
        others.push(saved)
      }
    }
    this.chats = [chat, ...others].sort(AiStore.newestFirst).slice(0, AI_MAX_CHATS)
    this.writeChats()
  }

  remove(id: string): void {
    const kept: SavedChat[] = []
    for (const chat of this.chats) {
      if (chat.id !== id) {
        kept.push(chat)
      }
    }
    this.chats = kept
    this.writeChats()
  }

  private static newestFirst(left: SavedChat, right: SavedChat): number {
    return right.updated - left.updated
  }

  private fixChoices(): void {
    if (!AI_PROVIDERS[this.provider]) {
      this.provider = AI_DEFAULT_PROVIDER
    }
    // A custom model id is kept when it has the allowed form.
    if (!AiStore.has(this.current.models, this.model) && !AI_MODEL_PATTERN.test(this.model)) {
      this.model = ''
    }
    if (!AiStore.has(this.current.efforts, this.effort)) {
      this.effort = ''
    }
  }

  /** True when `model` is one of the listed models of the provider. */
  knows(model: string): boolean {
    return AiStore.has(this.current.models, model)
  }

  private static has(choices: AiChoice[], value: string): boolean {
    for (const choice of choices) {
      if (choice.value === value) {
        return true
      }
    }
    return false
  }

  private writeChats(): void {
    this.storage.setItem(AI_CHATS_STORAGE_KEY, JSON.stringify(this.chats))
  }

  /** Saved JSON. Broken data counts as nothing saved. */
  private read(key: string): unknown {
    try {
      return JSON.parse(this.storage.getItem(key) ?? 'null')
    } catch {
      return null
    }
  }
}
