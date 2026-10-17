import { format } from 'sql-formatter'
import { SQL_FORMAT_LANGUAGES } from '../constants/sql'
import type { Driver } from '../types'

/** Formats SQL in the dialect of a database kind. */
export class SqlFormat {
  private driver: Driver | undefined

  /** Without a driver it uses plain standard SQL. */
  constructor(driver: Driver | undefined) {
    this.driver = driver
  }

  /** The formatted SQL. Throws when the SQL cannot be parsed. */
  format(sql: string): string {
    const language = this.driver ? SQL_FORMAT_LANGUAGES[this.driver] : 'sql'
    return format(sql, { language })
  }
}
