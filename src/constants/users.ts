import type { AccessLevel, GrantableAccess } from '../types'

export const ACCESS_LABELS: Record<AccessLevel, string> = {
  none: 'None',
  read: 'Read',
  write: 'Read + Write',
  admin: 'Admin',
  custom: 'Custom',
}

/** The levels a user can be set to, in the order of the select. */
export const GRANTABLE_ACCESS: GrantableAccess[] = ['none', 'read', 'write']

/** From the fewest rights to the most. A change to an earlier level takes rights away. */
export const ACCESS_ORDER: AccessLevel[] = ['none', 'read', 'write', 'custom', 'admin']

/** The MySQL account host that means "from any host". */
export const DEFAULT_USER_HOST = '%'
