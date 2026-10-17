# TODO

## Later

- Fine-grained user rights per table in the Users view. Today access is only None, Read, or Read + Write for the whole database.
- Add AI agent integration with claude

## SQL Server

These parts were left out of the first SQL Server support.

- Backup. The Backup button is hidden for SQL Server. It needs `sqlpackage` or a dump made by the app itself.
- The Users view. Logins, database users and roles work in another way than in MySQL and PostgreSQL.
  When it comes, add the Query stats right too: `GRANT VIEW SERVER STATE TO <login>` / `REVOKE VIEW SERVER STATE FROM <login>` (in `master`).
- `PRINT` and other info messages. The driver drops them, so the query editor does not show them.
- Columns of type `xml`, `text`, `ntext`, `image`, `sql_variant` and CLR types (`geography`, `geometry`, `hierarchyid`) in the query editor. The driver cannot read them, so the query fails with a hint to cast the column. Browsing a table casts them by itself. Other CLR types fail there too.
- Cancel ends the whole session with `KILL` (an open transaction is rolled back) and needs the `ALTER ANY CONNECTION` right. A TDS attention packet in the driver would stop only the query.
- A setting to check the server certificate. Today the traffic is encrypted, but the certificate is not checked (like the other databases in the app).
- `varchar` text in a code page other than Latin-1 (Cyrillic, Greek, ...) shows wrong letters.
- An exported table with an `IDENTITY` column cannot be imported back as it is: the `INSERT` needs `SET IDENTITY_INSERT ... ON`.
- A table is always created in the default schema of the login (mostly `dbo`).
