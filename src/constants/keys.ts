import type { ReferentialAction } from '../types'

/** The rules a foreign key can have for ON DELETE and ON UPDATE. */
export const REFERENTIAL_ACTIONS: ReferentialAction[] = [
  'NO ACTION',
  'RESTRICT',
  'CASCADE',
  'SET NULL',
  'SET DEFAULT',
]

/** The rule a new foreign key starts with. It is the default of all three databases. */
export const DEFAULT_REFERENTIAL_ACTION: ReferentialAction = 'NO ACTION'
