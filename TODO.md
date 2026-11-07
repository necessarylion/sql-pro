# TODO

Features that paid SQL clients have. Do them in this order. Remove an item when it is done.

1. **Data compare.** Compare the rows of one table in two databases. Make the `INSERT` / `UPDATE` / `DELETE` SQL that fixes the differences.
2. **Visual query plan.** Show EXPLAIN as a tree. Mark the slow steps.
3. **Charts from a query result.** Show a result as a bar, line or pie chart.
4. **JSON cell viewer.** Show a JSON value as a tree you can fold, and edit it there.
5. **Test data generator.** Fill a table with fake rows (names, emails, dates). Respect the column types and foreign keys.
6. **Scheduled backups.** Run a backup every day or week. Keep the last N files.
7. **Excel import.** Import an `.xlsx` file into a table, like CSV import.
8. **More databases.** Redis and MongoDB.
9. **Sync saved connections** across machines. Passwords must stay in the OS keychain.

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
