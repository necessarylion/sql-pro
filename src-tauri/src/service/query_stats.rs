//! The Query stats view: the statements that ran on the server, from all clients.
//! It only reads. It never changes a setting of the server.

use super::ConnectionService;
use crate::constants::{QUERY_STATS_MAX_DAYS, QUERY_STATS_MIN_DAYS, QUERY_STATS_NOT_SET_UP};
use crate::error::{AppError, AppResult};
use crate::model::QueryStat;

impl ConnectionService {
    /// The query shapes of the last `days` days, the most total time first.
    /// When the server does not collect stats, the error says how to turn them on.
    pub async fn query_stats(&self, id: &str, days: u32) -> AppResult<Vec<QueryStat>> {
        let repository = self.repository(id)?;
        let sql =
            repository.query_stats_sql(days.clamp(QUERY_STATS_MIN_DAYS, QUERY_STATS_MAX_DAYS))?;
        let setup = repository.query_stats_setup();
        if let Some(check) = repository.query_stats_on_sql() {
            let sets = repository
                .query(check)
                .await
                .map_err(|error| hint(setup, error))?;
            let on = sets
                .first()
                .and_then(|set| set.rows.first())
                .and_then(|row| row.first());
            if on.cloned().flatten().as_deref() != Some("1") {
                return Err(AppError::Message(format!(
                    "{QUERY_STATS_NOT_SET_UP} {setup}"
                )));
            }
        }
        let sets = repository
            .query(&sql)
            .await
            .map_err(|error| hint(setup, error))?;
        let rows = sets
            .into_iter()
            .next()
            .map(|set| set.rows)
            .unwrap_or_default();
        let mut stats = Vec::with_capacity(rows.len());
        for row in rows {
            let mut cells = row.into_iter();
            let mut next = || cells.next().flatten();
            let Some(query) = next() else {
                continue;
            };
            stats.push(QueryStat {
                query,
                database: next(),
                runs: next().and_then(|text| text.parse().ok()).unwrap_or(0),
                last_run: next(),
                total_ms: number(next()).unwrap_or(0.0),
                average_ms: number(next()).unwrap_or(0.0),
                max_ms: number(next()),
                rows: next().and_then(|text| text.parse().ok()),
            });
        }
        Ok(stats)
    }
}

fn number(text: Option<String>) -> Option<f64> {
    text.and_then(|text| text.parse().ok())
}

/// A server error here most often means the stats are off or not allowed. So it says how
/// to fix that, and keeps the text of the server. Other errors stay as they are.
/// The text starts with `QUERY_STATS_NOT_SET_UP`, so the view shows its setup guide.
fn hint(setup: &str, error: AppError) -> AppError {
    match error {
        AppError::Database(sqlx::Error::Database(_)) => {
            AppError::Message(format!("{QUERY_STATS_NOT_SET_UP} {setup} ({error})"))
        }
        other => other,
    }
}
