import { MSSQL, MySQL, PostgreSQL, SQLite, type SQLDialect } from '@codemirror/lang-sql'
import type { Driver } from '../types'

/** The SQL dialect of each database kind, for the editor and for highlighted SQL. */
export const SQL_DIALECTS: Record<Driver, SQLDialect> = {
  mysql: MySQL,
  postgres: PostgreSQL,
  mssql: MSSQL,
  sqlite: SQLite,
}
