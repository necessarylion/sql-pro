/** The column order the user dragged in the Content view, for all tables, is kept under this key. */
export const COLUMN_ORDER_STORAGE_KEY = 'sql-pro.column-order'

/** A column `extra` that means the database fills the value itself: MySQL, PostgreSQL and SQL Server. */
export const AUTO_COLUMN_EXTRA = /auto_increment|identity/i

/** A generated (computed) column. It takes no value. MySQL `DEFAULT_GENERATED` is only a default, so it does not count. */
export const GENERATED_COLUMN_EXTRA = /(?<!default_)generated/i

/** A PostgreSQL `serial` column has a sequence as its default. */
export const SEQUENCE_DEFAULT = /^nextval\(/i

/** In SQLite a primary key of this type, alone, numbers the rows itself. */
export const SQLITE_ROWID_TYPE = 'integer'
