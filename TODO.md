# TODO

Features that paid SQL clients have. Do them in this order. Remove an item when it is done.

1. **Schema compare and sync.** Compare two databases (for example dev and production). Show what is different, then make the `ALTER` SQL that makes one the same as the other.
2. **Data compare.** Compare the rows of one table in two databases. Make the `INSERT` / `UPDATE` / `DELETE` SQL that fixes the differences.
3. **Visual query plan.** Show EXPLAIN as a tree. Mark the slow steps.
4. **Charts from a query result.** Show a result as a bar, line or pie chart.
5. **JSON cell viewer.** Show a JSON value as a tree you can fold, and edit it there.
6. **Test data generator.** Fill a table with fake rows (names, emails, dates). Respect the column types and foreign keys.
7. **Scheduled backups.** Run a backup every day or week. Keep the last N files.
8. **Excel import.** Import an `.xlsx` file into a table, like CSV import.
9. **More databases.** Redis and MongoDB.
10. **Sync saved connections** across machines. Passwords must stay in the OS keychain.
