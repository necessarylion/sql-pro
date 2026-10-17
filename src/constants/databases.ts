/**
 * The system databases a tab switches to before it drops its current database:
 * `postgres` in PostgreSQL and `master` in SQL Server.
 */
export const FALLBACK_DATABASES: readonly string[] = ['postgres', 'master']

/**
 * Common MySQL / MariaDB collations for each charset. The "New database" dialog suggests them.
 * The keys are also the charset suggestions. It is a short list, not every value the server knows.
 */
export const MYSQL_COLLATIONS: Readonly<Record<string, readonly string[]>> = {
  utf8mb4: [
    'utf8mb4_0900_ai_ci',
    'utf8mb4_0900_as_cs',
    'utf8mb4_0900_bin',
    'utf8mb4_unicode_ci',
    'utf8mb4_unicode_520_ci',
    'utf8mb4_general_ci',
    'utf8mb4_bin',
  ],
  utf8mb3: ['utf8mb3_general_ci', 'utf8mb3_unicode_ci', 'utf8mb3_bin'],
  latin1: ['latin1_swedish_ci', 'latin1_general_ci', 'latin1_general_cs', 'latin1_bin'],
  ascii: ['ascii_general_ci', 'ascii_bin'],
  binary: ['binary'],
  utf16: ['utf16_general_ci', 'utf16_unicode_ci', 'utf16_bin'],
  utf32: ['utf32_general_ci', 'utf32_unicode_ci', 'utf32_bin'],
  ucs2: ['ucs2_general_ci', 'ucs2_unicode_ci', 'ucs2_bin'],
}

/**
 * The value of the "New database…" entry in the database picker. A database name cannot hold
 * a NUL character, so it never matches a real database.
 */
export const NEW_DATABASE_OPTION = '\u0000new'

/** The value of the "Drop database…" entry in the database picker. NUL keeps it apart, as above. */
export const DROP_DATABASE_OPTION = '\u0000drop'
