/** The environments that have their own colour. Any other name is a custom one. */
export const BUILT_IN_ENVIRONMENTS = ['local', 'dev', 'qa', 'prod'] as const

export type BuiltInEnvironment = (typeof BUILT_IN_ENVIRONMENTS)[number]

/** `none` = no name at all (a tab that is not connected). Not a button in the form. `custom` = a name the user typed. */
export type EnvironmentKind = 'none' | BuiltInEnvironment | 'custom'

/** The buttons of the environment switch in the connection form, in order. */
export const ENVIRONMENT_KINDS: EnvironmentKind[] = [...BUILT_IN_ENVIRONMENTS, 'custom']

/** The environment of a new connection, and of one with no name. */
export const DEFAULT_ENVIRONMENT = 'local'

export const ENVIRONMENT_MAX_LENGTH = 16

/** Every character that is not allowed in an environment name. */
export const ENVIRONMENT_BAD_CHARACTERS = /[^a-z0-9_-]/g

export const ENVIRONMENT_HINT = `Lower case letters, numbers, - and _. Up to ${ENVIRONMENT_MAX_LENGTH} characters.`
