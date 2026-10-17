import { MSSQL, MySQL, PostgreSQL, SQLite, type SQLDialect } from '@codemirror/lang-sql'
import type { SqlLanguage } from 'sql-formatter'
import type { Driver } from '../types'

/** The SQL dialect of each database kind, for the editor and for highlighted SQL. */
export const SQL_DIALECTS: Record<Driver, SQLDialect> = {
  mysql: MySQL,
  postgres: PostgreSQL,
  mssql: MSSQL,
  sqlite: SQLite,
}

/** The sql-formatter language of each database kind. `mysql` also covers MariaDB. */
export const SQL_FORMAT_LANGUAGES: Record<Driver, SqlLanguage> = {
  mysql: 'mysql',
  postgres: 'postgresql',
  mssql: 'transactsql',
  sqlite: 'sqlite',
}
