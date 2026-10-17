/** The "last N days" choices of the Query stats view. Any other number can be typed in. */
export const QUERY_STATS_DAY_CHOICES = [1, 7, 30]

/** The days the Query stats view starts with. */
export const QUERY_STATS_DEFAULT_DAYS = 7

/** The backend uses the same bounds. */
export const QUERY_STATS_MIN_DAYS = 1
export const QUERY_STATS_MAX_DAYS = 3650

/**
 * The start of the backend error when the server does not collect query stats, or the
 * account may not read them. Keep it the same as `QUERY_STATS_NOT_SET_UP` in the backend.
 */
export const QUERY_STATS_NOT_SET_UP = 'Query stats are not set up.'

/** Another way to do a step. `run` works like the one of the step. */
export interface QueryStatsOption {
  text: string
  code: string
  run?: true
}

/**
 * One step of the setup guide. `code` can always be copied. With `run`, it is SQL that the app
 * can also run, but only when the user clicks Run and then OK in the warning.
 */
export interface QueryStatsStep {
  text: string
  /** The text in place of `text` when the MySQL account of the connection is not known. */
  unknownAccountText?: string
  code?: string
  /** True only for SQL. Config lines, Docker commands and restarts cannot run. */
  run?: true
  /** Other ways to do the same step. They are part of the step, not steps of their own. */
  options?: QueryStatsOption[]
}

/** The title of the warning before a setup snippet runs. */
export const QUERY_STATS_RUN_TITLE = 'Run setup SQL'

/** The warning before a setup snippet runs. The SQL follows it. */
export const QUERY_STATS_RUN_WARNING =
  'This SQL changes the server. It runs on the connection of this tab. Run it?'

/** The status after a setup snippet ran. */
export const QUERY_STATS_RUN_DONE = 'Done'

/** `{user}` in a snippet becomes the user name of the connection. */
export const QUERY_STATS_USER = '{user}'

/** `{account}` in a MySQL snippet becomes the quoted account, like 'app'@'%'. */
export const QUERY_STATS_ACCOUNT = '{account}'

/** Reads the MySQL account the server matched for this connection, like app@%. It only reads. */
export const MYSQL_CURRENT_USER_SQL = 'SELECT CURRENT_USER()'

/** How to turn on query stats, per database kind. MariaDB uses the MySQL steps. */
export const QUERY_STATS_GUIDES: Record<'mysql' | 'postgres' | 'mssql', QueryStatsStep[]> = {
  mysql: [
    {
      text: 'Turn on performance_schema. Add this to my.cnf, under [mysqld].',
      code: '[mysqld]\nperformance_schema = ON',
    },
    { text: 'Restart the server.' },
    {
      text: 'Check that it is on. This must return 1.',
      code: 'SELECT @@performance_schema;',
      run: true,
    },
    {
      text: 'Let your account read the stats. An admin runs this, or turns on "Query stats" for the user on the Users screen.',
      unknownAccountText:
        'Let your account read the stats. An admin runs this, or turns on "Query stats" for the user on the Users screen. Put the host of your account in place of host.',
      code: 'GRANT SELECT ON performance_schema.* TO {account};',
      run: true,
    },
  ],
  postgres: [
    {
      text: 'Load pg_stat_statements when the server starts. Add this line to postgresql.conf.',
      code: "shared_preload_libraries = 'pg_stat_statements'",
      options: [
        {
          text: 'Or run this as a superuser. It still needs the restart in the next step.',
          code: "ALTER SYSTEM SET shared_preload_libraries = 'pg_stat_statements';",
          run: true,
        },
        {
          text: 'Or on Docker, start the server with this command in docker-compose.yml:',
          code: 'command: postgres -c shared_preload_libraries=pg_stat_statements',
        },
      ],
    },
    { text: 'Restart the server.' },
    {
      text: 'Create the extension in this database.',
      code: 'CREATE EXTENSION IF NOT EXISTS pg_stat_statements;',
      run: true,
    },
    {
      text: 'To see the queries of other users, a user who is not a superuser needs this role. An admin runs this, or turns on "Query stats" for the user on the Users screen.',
      code: 'GRANT pg_read_all_stats TO {user};',
      run: true,
    },
  ],
  mssql: [
    {
      text: 'Give your login the VIEW SERVER STATE permission. An admin runs this.',
      code: 'GRANT VIEW SERVER STATE TO [{user}];',
      run: true,
      options: [
        {
          text: 'Or on SQL Server 2022 and later, this smaller permission is enough:',
          code: 'GRANT VIEW SERVER PERFORMANCE STATE TO [{user}];',
          run: true,
        },
        {
          text: 'Or on Azure SQL Database, give the database user this permission:',
          code: 'GRANT VIEW DATABASE STATE TO [{user}];',
          run: true,
        },
      ],
    },
  ],
}
