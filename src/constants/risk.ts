/** A statement that starts with one of these words is always risky. */
export const ALWAYS_RISKY_KEYWORDS = ['DROP', 'TRUNCATE', 'ALTER', 'RENAME']

/** A statement that starts with one of these words is risky when it has no WHERE. */
export const NEEDS_WHERE_KEYWORDS = ['DELETE', 'UPDATE']

/** How much of a risky statement the warning shows. */
export const RISK_PREVIEW_LENGTH = 60

/** How many risky statements the warning lists. The rest is "and N more". */
export const RISK_LIST_LIMIT = 5
