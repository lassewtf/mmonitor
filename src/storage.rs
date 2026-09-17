use std::{collections::BTreeMap, error::Error, path::Path, time::Duration};

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

use crate::{CheckResult, Execution, config::StoreStrategy};

pub(crate) type StorageResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

const FIVE_MINUTES_MS: i64 = 5 * 60 * 1000;
const ONE_HOUR_MS: i64 = 60 * 60 * 1000;
const SEVEN_DAYS_MS: i64 = 7 * 24 * ONE_HOUR_MS;
const THIRTY_DAYS_MS: i64 = 30 * 24 * ONE_HOUR_MS;

pub(crate) fn open(path: &Path) -> StorageResult<Connection> {
    let connection = Connection::open(path)?;
    connection.busy_timeout(Duration::ZERO)?;
    connection.pragma_update(None, "foreign_keys", "ON")?;

    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    match version {
        0 => {
            connection.pragma_update(None, "journal_mode", "WAL")?;
            migrate_initial(&connection)?;
        }
        1 => connection.pragma_update(None, "journal_mode", "WAL")?,
        other => return Err(format!("unsupported database schema version: {other}").into()),
    }
    Ok(connection)
}

fn migrate_initial(connection: &Connection) -> StorageResult<()> {
    connection.execute_batch(
        "BEGIN IMMEDIATE;
         CREATE TABLE check_state (
             check_id TEXT PRIMARY KEY,
             last_attempt_at INTEGER NOT NULL,
             last_success_at INTEGER,
             last_execution TEXT NOT NULL,
             last_error TEXT,
             last_snapshot TEXT NOT NULL
         );
         CREATE TABLE runs (
             id INTEGER PRIMARY KEY,
             check_id TEXT NOT NULL,
             observed_at INTEGER NOT NULL,
             execution TEXT NOT NULL,
             exit_code INTEGER,
             duration_ms INTEGER NOT NULL,
             error TEXT
         );
         CREATE INDEX runs_observed_at ON runs(observed_at);
         CREATE TABLE samples (
             run_id INTEGER NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
             metric_name TEXT NOT NULL,
             value REAL NOT NULL,
             unit TEXT
         );
         CREATE TABLE facts (
             run_id INTEGER NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
             fact_name TEXT NOT NULL,
             value TEXT NOT NULL
         );
         CREATE TABLE rollups (
             check_id TEXT NOT NULL,
             metric_name TEXT NOT NULL,
             unit TEXT NOT NULL,
             resolution_seconds INTEGER NOT NULL,
             bucket_start INTEGER NOT NULL,
             minimum REAL NOT NULL,
             maximum REAL NOT NULL,
             sum REAL NOT NULL,
             count INTEGER NOT NULL,
             first REAL NOT NULL,
             last REAL NOT NULL,
             PRIMARY KEY (check_id, metric_name, unit, resolution_seconds, bucket_start)
         );
         CREATE TABLE maintenance (
             name TEXT PRIMARY KEY,
             completed_at INTEGER NOT NULL
         );
         PRAGMA user_version = 1;
         COMMIT;",
    )?;
    Ok(())
}

pub(crate) fn begin_collection(connection: &mut Connection) -> StorageResult<Transaction<'_>> {
    Ok(connection.transaction_with_behavior(TransactionBehavior::Immediate)?)
}

pub(crate) fn is_due(
    transaction: &Transaction<'_>,
    check_id: &str,
    now_ms: i64,
    interval_seconds: u64,
) -> StorageResult<bool> {
    let last_attempt: Option<i64> = transaction
        .query_row(
            "SELECT last_attempt_at FROM check_state WHERE check_id = ?1",
            [check_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(last_attempt.is_none_or(|last| {
        now_ms < last || now_ms.saturating_sub(last) >= interval_seconds as i64 * 1000
    }))
}

pub(crate) fn record(
    transaction: &Transaction<'_>,
    result: &CheckResult,
    strategy: StoreStrategy,
    observed_at: i64,
) -> StorageResult<bool> {
    let snapshot = semantic_snapshot(result)?;
    let previous: Option<String> = transaction
        .query_row(
            "SELECT last_snapshot FROM check_state WHERE check_id = ?1",
            [&result.id],
            |row| row.get(0),
        )
        .optional()?;
    let store = strategy == StoreStrategy::Always || previous.as_deref() != Some(&snapshot);

    if store {
        transaction.execute(
            "INSERT INTO runs (check_id, observed_at, execution, exit_code, duration_ms, error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                result.id,
                observed_at,
                execution_name(result.execution),
                result.exit_code,
                result.duration_ms as i64,
                result.error,
            ],
        )?;
        let run_id = transaction.last_insert_rowid();
        for metric in &result.metrics {
            transaction.execute(
                "INSERT INTO samples (run_id, metric_name, value, unit) VALUES (?1, ?2, ?3, ?4)",
                params![run_id, metric.name, metric.value, metric.unit],
            )?;
        }
        for fact in &result.facts {
            transaction.execute(
                "INSERT INTO facts (run_id, fact_name, value) VALUES (?1, ?2, ?3)",
                params![run_id, fact.name, fact.value],
            )?;
        }
    }

    let last_success = (result.execution == Execution::Completed).then_some(observed_at);
    transaction.execute(
        "INSERT INTO check_state (
             check_id, last_attempt_at, last_success_at, last_execution, last_error, last_snapshot
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(check_id) DO UPDATE SET
             last_attempt_at = excluded.last_attempt_at,
             last_success_at = COALESCE(excluded.last_success_at, check_state.last_success_at),
             last_execution = excluded.last_execution,
             last_error = excluded.last_error,
             last_snapshot = excluded.last_snapshot",
        params![
            result.id,
            observed_at,
            last_success,
            execution_name(result.execution),
            result.error,
            snapshot,
        ],
    )?;
    Ok(store)
}

fn semantic_snapshot(result: &CheckResult) -> StorageResult<String> {
    let mut metrics = result.metrics.iter().collect::<Vec<_>>();
    metrics.sort_by(|left, right| (&left.name, &left.unit).cmp(&(&right.name, &right.unit)));
    let mut facts = result.facts.iter().collect::<Vec<_>>();
    facts.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(serde_json::to_string(&serde_json::json!({
        "execution": execution_name(result.execution),
        "error": result.error,
        "metrics": metrics,
        "facts": facts,
    }))?)
}

fn execution_name(execution: Execution) -> &'static str {
    match execution {
        Execution::Completed => "completed",
        Execution::TimedOut => "timed_out",
        Execution::SpawnFailed => "spawn_failed",
        Execution::InvalidOutput => "invalid_output",
    }
}

#[derive(Clone)]
struct Aggregate {
    minimum: f64,
    maximum: f64,
    sum: f64,
    count: i64,
    first: f64,
    last: f64,
}

impl Aggregate {
    fn raw(value: f64) -> Self {
        Self {
            minimum: value,
            maximum: value,
            sum: value,
            count: 1,
            first: value,
            last: value,
        }
    }

    fn append(&mut self, other: &Self) {
        self.minimum = self.minimum.min(other.minimum);
        self.maximum = self.maximum.max(other.maximum);
        self.sum += other.sum;
        self.count += other.count;
        self.last = other.last;
    }
}

type RollupKey = (String, String, String, i64);

pub(crate) fn compact_if_due(transaction: &Transaction<'_>, now_ms: i64) -> StorageResult<bool> {
    let last: Option<i64> = transaction
        .query_row(
            "SELECT completed_at FROM maintenance WHERE name = 'compact'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if last.is_some_and(|last| now_ms >= last && now_ms - last < ONE_HOUR_MS) {
        return Ok(false);
    }

    compact_raw(transaction, now_ms)?;
    compact_five_minutes(transaction, now_ms)?;
    transaction.execute(
        "INSERT INTO maintenance (name, completed_at) VALUES ('compact', ?1)
         ON CONFLICT(name) DO UPDATE SET completed_at = excluded.completed_at",
        [now_ms],
    )?;
    Ok(true)
}

fn compact_raw(transaction: &Transaction<'_>, now_ms: i64) -> StorageResult<()> {
    let cutoff = bucket_start(now_ms - SEVEN_DAYS_MS, FIVE_MINUTES_MS);
    let rows = {
        let mut statement = transaction.prepare(
            "SELECT r.check_id, r.observed_at, s.metric_name, s.value, COALESCE(s.unit, '')
             FROM samples s JOIN runs r ON r.id = s.run_id
             WHERE r.observed_at < ?1
             ORDER BY r.check_id, s.metric_name, COALESCE(s.unit, ''), r.observed_at, r.id",
        )?;
        let mapped = statement.query_map([cutoff], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, f64>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?;
        mapped.collect::<Result<Vec<_>, _>>()?
    };

    let mut aggregates = BTreeMap::<RollupKey, Aggregate>::new();
    for (check_id, observed_at, name, value, unit) in rows {
        let key = (
            check_id,
            name,
            unit,
            bucket_start(observed_at, FIVE_MINUTES_MS),
        );
        aggregates
            .entry(key)
            .and_modify(|aggregate| aggregate.append(&Aggregate::raw(value)))
            .or_insert_with(|| Aggregate::raw(value));
    }
    write_rollups(transaction, 300, aggregates)?;

    transaction.execute(
        "DELETE FROM samples WHERE run_id IN (
             SELECT id FROM runs WHERE observed_at < ?1
         )",
        [cutoff],
    )?;
    transaction.execute(
        "DELETE FROM runs
         WHERE observed_at < ?1 AND execution = 'completed'
           AND NOT EXISTS (SELECT 1 FROM samples WHERE samples.run_id = runs.id)
           AND NOT EXISTS (SELECT 1 FROM facts WHERE facts.run_id = runs.id)",
        [cutoff],
    )?;
    Ok(())
}

fn compact_five_minutes(transaction: &Transaction<'_>, now_ms: i64) -> StorageResult<()> {
    let cutoff = bucket_start(now_ms - THIRTY_DAYS_MS, ONE_HOUR_MS);
    let rows = {
        let mut statement = transaction.prepare(
            "SELECT check_id, metric_name, unit, bucket_start,
                    minimum, maximum, sum, count, first, last
             FROM rollups
             WHERE resolution_seconds = 300 AND bucket_start < ?1
             ORDER BY check_id, metric_name, unit, bucket_start",
        )?;
        let mapped = statement.query_map([cutoff], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                Aggregate {
                    minimum: row.get(4)?,
                    maximum: row.get(5)?,
                    sum: row.get(6)?,
                    count: row.get(7)?,
                    first: row.get(8)?,
                    last: row.get(9)?,
                },
            ))
        })?;
        mapped.collect::<Result<Vec<_>, _>>()?
    };

    let mut aggregates = BTreeMap::<RollupKey, Aggregate>::new();
    for (check_id, name, unit, start, aggregate) in rows {
        let key = (check_id, name, unit, bucket_start(start, ONE_HOUR_MS));
        aggregates
            .entry(key)
            .and_modify(|existing| existing.append(&aggregate))
            .or_insert(aggregate);
    }
    write_rollups(transaction, 3600, aggregates)?;
    transaction.execute(
        "DELETE FROM rollups WHERE resolution_seconds = 300 AND bucket_start < ?1",
        [cutoff],
    )?;
    Ok(())
}

fn write_rollups(
    transaction: &Transaction<'_>,
    resolution_seconds: i64,
    aggregates: BTreeMap<RollupKey, Aggregate>,
) -> StorageResult<()> {
    for ((check_id, name, unit, start), aggregate) in aggregates {
        transaction.execute(
            "INSERT INTO rollups (
                 check_id, metric_name, unit, resolution_seconds, bucket_start,
                 minimum, maximum, sum, count, first, last
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(check_id, metric_name, unit, resolution_seconds, bucket_start)
             DO UPDATE SET
                 minimum = excluded.minimum,
                 maximum = excluded.maximum,
                 sum = excluded.sum,
                 count = excluded.count,
                 first = excluded.first,
                 last = excluded.last",
            params![
                check_id,
                name,
                unit,
                resolution_seconds,
                start,
                aggregate.minimum,
                aggregate.maximum,
                aggregate.sum,
                aggregate.count,
                aggregate.first,
                aggregate.last,
            ],
        )?;
    }
    Ok(())
}

fn bucket_start(timestamp: i64, width: i64) -> i64 {
    timestamp.div_euclid(width) * width
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::{CheckResult, Fact, Metric};

    use super::*;

    fn database_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("mmonitor-{name}-{}.sqlite3", std::process::id()))
    }

    fn result(version: &str) -> CheckResult {
        CheckResult {
            id: "macos_version".to_owned(),
            execution: Execution::Completed,
            exit_code: Some(0),
            metrics: Vec::new(),
            facts: vec![Fact {
                name: "os.version".to_owned(),
                value: version.to_owned(),
            }],
            stdout: String::new(),
            stderr: String::new(),
            duration_ms: 1,
            error: None,
        }
    }

    #[test]
    fn stores_only_changed_snapshots() {
        let path = database_path("changes");
        let _ = fs::remove_file(&path);
        let mut connection = open(&path).unwrap();
        let transaction = begin_collection(&mut connection).unwrap();

        assert!(record(&transaction, &result("27.0"), StoreStrategy::OnChange, 1).unwrap());
        assert!(!record(&transaction, &result("27.0"), StoreStrategy::OnChange, 2).unwrap());
        assert!(record(&transaction, &result("27.1"), StoreStrategy::OnChange, 3).unwrap());
        let mut failed = result("27.1");
        failed.execution = Execution::SpawnFailed;
        failed.error = Some("unavailable".to_owned());
        assert!(record(&transaction, &failed, StoreStrategy::OnChange, 4).unwrap());
        assert!(!record(&transaction, &failed, StoreStrategy::OnChange, 5).unwrap());
        assert!(record(&transaction, &result("27.1"), StoreStrategy::OnChange, 6).unwrap());
        transaction.commit().unwrap();

        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM runs", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            4
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT last_attempt_at FROM check_state WHERE check_id = 'macos_version'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            6
        );
        drop(connection);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn rejects_parallel_collectors() {
        let path = database_path("lock");
        let _ = fs::remove_file(&path);
        let mut first = open(&path).unwrap();
        let mut second = open(&path).unwrap();
        let transaction = begin_collection(&mut first).unwrap();

        assert!(begin_collection(&mut second).is_err());

        transaction.rollback().unwrap();
        drop(first);
        drop(second);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn compacts_raw_samples_without_touching_facts() {
        let path = database_path("compact");
        let _ = fs::remove_file(&path);
        let mut connection = open(&path).unwrap();
        let now = 40 * 24 * ONE_HOUR_MS;
        let old = now - 8 * 24 * ONE_HOUR_MS;
        let transaction = begin_collection(&mut connection).unwrap();

        for (offset, value) in [(0, 2.0), (1000, 6.0)] {
            let check = CheckResult {
                id: "memory".to_owned(),
                execution: Execution::Completed,
                exit_code: Some(0),
                metrics: vec![Metric {
                    name: "memory.used".to_owned(),
                    value,
                    unit: Some("B".to_owned()),
                }],
                facts: Vec::new(),
                stdout: String::new(),
                stderr: String::new(),
                duration_ms: 1,
                error: None,
            };
            record(&transaction, &check, StoreStrategy::Always, old + offset).unwrap();
        }
        record(&transaction, &result("27.0"), StoreStrategy::Always, old).unwrap();
        compact_if_due(&transaction, now).unwrap();
        transaction.commit().unwrap();

        let aggregate = connection
            .query_row(
                "SELECT minimum, maximum, sum, count, first, last FROM rollups
                 WHERE resolution_seconds = 300",
                [],
                |row| {
                    Ok((
                        row.get::<_, f64>(0)?,
                        row.get::<_, f64>(1)?,
                        row.get::<_, f64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, f64>(4)?,
                        row.get::<_, f64>(5)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(aggregate, (2.0, 6.0, 8.0, 2, 2.0, 6.0));
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM facts", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );
        drop(connection);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn compacts_five_minute_rollups_into_weighted_hours() {
        let path = database_path("hourly");
        let _ = fs::remove_file(&path);
        let mut connection = open(&path).unwrap();
        let now = 40 * 24 * ONE_HOUR_MS;
        let start = bucket_start(now - 31 * 24 * ONE_HOUR_MS, ONE_HOUR_MS);
        let transaction = begin_collection(&mut connection).unwrap();
        transaction
            .execute(
                "INSERT INTO rollups VALUES
                 ('memory', 'memory.used', 'B', 300, ?1, 1, 3, 4, 2, 1, 3),
                 ('memory', 'memory.used', 'B', 300, ?2, 5, 5, 5, 1, 5, 5)",
                params![start, start + FIVE_MINUTES_MS],
            )
            .unwrap();

        compact_if_due(&transaction, now).unwrap();
        let aggregate = transaction
            .query_row(
                "SELECT minimum, maximum, sum, count, first, last FROM rollups
                 WHERE resolution_seconds = 3600",
                [],
                |row| {
                    Ok((
                        row.get::<_, f64>(0)?,
                        row.get::<_, f64>(1)?,
                        row.get::<_, f64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, f64>(4)?,
                        row.get::<_, f64>(5)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(aggregate, (1.0, 5.0, 9.0, 3, 1.0, 5.0));
        transaction.commit().unwrap();
        drop(connection);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn rejects_unknown_schema_versions() {
        let path = database_path("version");
        let _ = fs::remove_file(&path);
        let connection = Connection::open(&path).unwrap();
        connection.pragma_update(None, "user_version", 99).unwrap();
        drop(connection);

        assert_eq!(
            open(&path).unwrap_err().to_string(),
            "unsupported database schema version: 99"
        );
        let _ = fs::remove_file(path);
    }
}
