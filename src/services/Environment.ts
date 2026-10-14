import {
  BUILT_IN_ENVIRONMENTS,
  DEFAULT_ENVIRONMENT,
  ENVIRONMENT_BAD_CHARACTERS,
  ENVIRONMENT_MAX_LENGTH,
} from '../constants/environment'
import type { EnvironmentKind } from '../constants/environment'

/**
 * The rules for the environment name of a connection, like `dev` or `prod`.
 * A name is lower case. '' means no environment (only a tab that is not connected).
 */
export class Environment {
  /**
   * Makes a clean name from what the user typed.
   * Upper case becomes lower case. Spaces and other bad characters are dropped.
   * A name that is too long is cut.
   */
  static clean(typed: unknown): string {
    if (typeof typed !== 'string') {
      // Saved connections from before this field have no name.
      return ''
    }
    return typed
      .toLowerCase()
      .replace(ENVIRONMENT_BAD_CHARACTERS, '')
      .slice(0, ENVIRONMENT_MAX_LENGTH)
  }

  /** Like `clean`, but an empty name becomes `local`. Use it for saved connections. */
  static orLocal(typed: unknown): string {
    return Environment.clean(typed) || DEFAULT_ENVIRONMENT
  }

  /** Which colour a name gets. */
  static kind(name: string): EnvironmentKind {
    const clean = Environment.clean(name)
    if (clean === '') {
      return 'none'
    }
    for (const builtIn of BUILT_IN_ENVIRONMENTS) {
      if (builtIn === clean) {
        return builtIn
      }
    }
    return 'custom'
  }
}
