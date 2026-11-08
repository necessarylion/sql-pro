/** The PostgreSQL versions the backup and restore dialog always lists, newest first. */
export const PG_TOOL_VERSIONS: readonly number[] = [18, 17, 16, 15, 14, 13, 12]

/**
 * The oldest version Homebrew still installs. Homebrew disabled postgresql@12 and @13, and
 * disables @14 on 2026-11-12. `brew install` fails for a disabled formula.
 */
export const HOMEBREW_OLDEST_PG_VERSION = 15

/** Postgres.app, for macOS. The backend allows the same links (`GUIDE_LINKS`). */
export const POSTGRES_APP_URL = 'https://postgresapp.com/'

/** The EDB installer for macOS. */
export const PG_MAC_DOWNLOAD_URL = 'https://www.postgresql.org/download/macosx/'

/** The official PostgreSQL apt repository for Debian and Ubuntu. */
export const PG_APT_URL = 'https://wiki.postgresql.org/wiki/Apt'

/** The official PostgreSQL dnf repository (PGDG) for RHEL and Fedora. */
export const PG_DNF_URL = 'https://www.postgresql.org/download/linux/redhat/'

/** The EDB installer for Windows. */
export const PG_WINDOWS_DOWNLOAD_URL = 'https://www.postgresql.org/download/windows/'

/** `navigator.platform` of Windows matches this. */
export const WINDOWS_PLATFORM_PATTERN = /win/i

/** The backup dialog starts on the custom format (.dump). It is smaller and pg_restore loads it faster. */
export const PG_BACKUP_CUSTOM_DEFAULT = true
