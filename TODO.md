# TODO

Features that paid SQL clients have. Do them in this order. Remove an item when it is done.

1. **Charts from a query result.** Show a result as a bar, line or pie chart.
2. **JSON cell viewer.** Show a JSON value as a tree you can fold, and edit it there.
3. **Test data generator.** Fill a table with fake rows (names, emails, dates). Respect the column types and foreign keys.
4. **Scheduled backups.** Run a backup every day or week. Keep the last N files.
5. **Excel import.** Import an `.xlsx` file into a table, like CSV import.
6. **More databases.** Redis and MongoDB.
7. **Sync saved connections** across machines. Passwords must stay in the OS keychain.

## Query plan: left out for now

The Query view shows Explain and Analyze as a tree (`service/query_plan.rs`). These parts are not done yet:

- SQLite has no Analyze. It gives no real times.
- SQL Server Analyze fails when the result of the query has a type the driver cannot read (`text`,
  `xml`, ...), like Run does. Cast those columns in the query.
- MySQL with `explain_json_format_version = 2` writes another JSON form. Then only the Raw tab shows.
- The share of a step is its own time or cost against the total. Loops, parallel workers and
  subplans are counted a little differently by each database, so a share is a hint, not exact.
- SQL Server times are in whole milliseconds, so fast steps show 0 ms.

## Schema compare: left out for now

The Compare view compares tables, columns, indexes and foreign keys. These parts are not done yet:

- Views, triggers and routines are not compared.
- A changed primary key is not found.
- A renamed table or column shows as a drop and an add.
- Two different kinds of database (like MySQL to PostgreSQL) cannot be compared.
- SQLite cannot change a column or a foreign key of an existing table. The SQL shows a warning for those.
- A new PostgreSQL or SQL Server table is built from its columns. Checks and comments are not copied. A `serial` column becomes `serial` again.
- An index on an expression, or with a `WHERE` part, is found but not copied. The SQL shows a warning.
- A changed auto increment, identity or generated rule is found but not changed. The SQL shows a warning.

## Data compare: left out for now

The Compare view in Data mode compares the rows of one table. These parts are not done yet:

- Only one table at a time.
- With more than 10 000 different rows there is no SQL.
- Columns the database fills itself are left out: generated columns, identity columns outside the key, and SQL Server `rowversion`.
- PostgreSQL does not move the sequence of an identity or serial column after the inserts. Set it by hand.
- When the two databases sort the key in a different order (another collation, like PostgreSQL `C` against `en_US.utf8`, or MySQL against MariaDB defaults), the rows cannot be matched page by page. The view says the order may differ, or more than 10 000 rows differ, and makes no SQL.
- SQL Server writes `geography`, `geometry`, `hierarchyid` and `sql_variant` values back from their text. A `sql_variant` then holds text, a `geography` gets the default SRID 4326, and a `geometry` gets SRID 0. A change of the SRID alone is not seen, because the compare uses `ToString()`.
- A SQL Server table with a `sql_variant` primary key cannot be compared.
- A table with a float key pages with `OFFSET`, which is slow deep into a big table.
- Run does not check how many rows each statement changed. A compare after the run shows what is left.
