import type { AiMode } from '../types'

/** The AI may run at most this many SQL statements for one question. */
export const AI_MAX_STEPS = 8

/** How many result rows we send back to the AI. */
export const AI_RESULT_ROWS = 50

/** A longer cell value is cut to this many characters before it goes to the AI. */
export const AI_CELL_MAX = 200

/** The title of the question before SQL from the AI changes data. */
export const AI_CONFIRM_TITLE = 'Run SQL from the AI?'

/** What the AI is told when the user said No. */
export const AI_REJECTED = 'The user rejected this query. It did not run.'

/** What the AI is told when it sends a write in read-only mode. */
export const AI_READ_ONLY_REJECTED =
  'Write queries are not allowed in read-only mode. This query did not run.'

/** The backend error when the old AI session cannot be found. Same text as `CLAUDE_SESSION_GONE` in Rust. */
export const AI_SESSION_GONE = 'The AI session is gone'

/** One choice in a select. The value '' means "no flag": the tool's own default. */
export interface AiChoice {
  value: string
  label: string
}

/** One AI tool, with the models and effort levels it takes. */
export interface AiProvider {
  label: string
  models: AiChoice[]
  efforts: AiChoice[]
}

/**
 * The AI tools the app can run, by id. A new tool only adds an entry here
 * (and the same id in `AI_PROVIDERS` in Rust).
 * The values must match the allow-lists in Rust (`CLAUDE_MODELS`, `CLAUDE_EFFORTS`).
 */
export const AI_PROVIDERS: Record<string, AiProvider> = {
  claude: {
    label: 'Claude',
    // The CLI cannot list its models, so these are known ones. "Custom…" takes any other id.
    models: [
      { value: '', label: 'Default model' },
      { value: 'opus', label: 'Opus (latest)' },
      { value: 'sonnet', label: 'Sonnet (latest)' },
      { value: 'haiku', label: 'Haiku (latest)' },
      { value: 'fable', label: 'Fable (latest)' },
      { value: 'claude-opus-5-5', label: 'Opus 5.5' },
      { value: 'claude-sonnet-5-5', label: 'Sonnet 5.5' },
      { value: 'claude-haiku-5-5', label: 'Haiku 5.5' },
      { value: 'claude-fable-5-1', label: 'Fable 5.1' },
      { value: 'claude-opus-4-7', label: 'Opus 4.7' },
      { value: 'claude-opus-4-6', label: 'Opus 4.6' },
      { value: 'claude-sonnet-4-6', label: 'Sonnet 4.6' },
    ],
    efforts: [
      { value: '', label: 'Default effort' },
      { value: 'low', label: 'Low' },
      { value: 'medium', label: 'Medium' },
      { value: 'high', label: 'High' },
      { value: 'xhigh', label: 'Extra high' },
      { value: 'max', label: 'Max' },
    ],
  },
}

export const AI_DEFAULT_PROVIDER = 'claude'

/** The select value of the "Custom…" model choice. It is never sent. */
export const AI_CUSTOM_MODEL = '__custom__'

/** A model id the user may type. Rust checks the same rule (`is_model_id`). Never starts with `-`. */
export const AI_MODEL_PATTERN = /^[a-z0-9][a-z0-9.\-[\]]{0,79}$/

/** What the app does with SQL that can change data. There is no mode that runs it without asking. */
export const AI_MODES: { value: AiMode; label: string }[] = [
  { value: 'ask', label: 'Ask before writes' },
  { value: 'readOnly', label: 'Read only' },
]

/** The saved choices of provider, model, effort and mode. */
export const AI_SETTINGS_STORAGE_KEY = 'sql-pro.ai-settings'

/** The saved chats. */
export const AI_CHATS_STORAGE_KEY = 'sql-pro.ai-chats'

/** Only this many chats are kept. The oldest go first. */
export const AI_MAX_CHATS = 50

/** The title of a saved chat is its first question, cut to this many characters. */
export const AI_TITLE_MAX = 60
