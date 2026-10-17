//! The Processes view: the connections of the server, and stopping one of them.

use super::ConnectionService;
use crate::constants::PROCESS_GONE;
use crate::error::AppResult;
use crate::model::DbProcess;

impl ConnectionService {
    /// The connections of the server. `SQLite` has none and gives an error.
    pub async fn list_processes(&self, id: &str) -> AppResult<Vec<DbProcess>> {
        let repository = self.repository(id)?;
        let sql = repository.processes_sql()?;
        let sets = repository.query(sql).await?;
        let rows = sets
            .into_iter()
            .next()
            .map(|set| set.rows)
            .unwrap_or_default();
        let mut processes = Vec::with_capacity(rows.len());
        for row in rows {
            let mut cells = row.into_iter();
            let mut next = || cells.next().flatten();
            let Some(process) = next().and_then(|text| text.parse().ok()) else {
                continue;
            };
            processes.push(DbProcess {
                id: process,
                user: next(),
                host: next(),
                database: next(),
                state: next(),
                // A clock change on the server can make it go below zero.
                seconds: next()
                    .and_then(|text| text.parse::<i64>().ok())
                    .map(|seconds| seconds.max(0)),
                query: next(),
                current: next().as_deref() == Some("1"),
            });
        }
        Ok(processes)
    }

    /// Stops a process of the list. The connection of this tab cannot be stopped here.
    /// `process` is a number, so it is safe inside the SQL.
    pub async fn kill_process(&self, id: &str, process: i64, query_only: bool) -> AppResult<()> {
        let listed = self.list_processes(id).await?;
        let Some(found) = listed.iter().find(|listed| listed.id == process) else {
            return Err(PROCESS_GONE.into());
        };
        if found.current {
            return Err("This is the connection of this tab. It cannot be stopped here.".into());
        }
        let repository = self.repository(id)?;
        let sql = repository.kill_sql(process, query_only)?;
        repository.query(&sql).await?;
        Ok(())
    }
}
