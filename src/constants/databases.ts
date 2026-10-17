/**
 * The system databases a tab switches to before it drops its current database:
 * `postgres` in PostgreSQL and `master` in SQL Server.
 */
export const FALLBACK_DATABASES: readonly string[] = ['postgres', 'master']
