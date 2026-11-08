/** The history of run SQL is saved under this key. */
export const QUERY_HISTORY_STORAGE_KEY = 'sql-pro.query-history'

/** The history keeps this many entries per database. Older ones are dropped. */
export const QUERY_HISTORY_MAX_LENGTH = 200

/** The history keeps this many entries over all databases, so the storage cannot grow without end. */
export const QUERY_HISTORY_TOTAL_MAX_LENGTH = 2000

/** The saved queries (named SQL) are saved under this key. */
export const SAVED_QUERIES_STORAGE_KEY = 'sql-pro.saved-queries'

/** A saved query name cannot be longer than this. */
export const SAVED_QUERY_NAME_MAX_LENGTH = 100
