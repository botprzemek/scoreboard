use std::error::Error;
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::adapters::data::Table;

use rusqlite::{Connection, params};

use super::provider::resolve_db_path;

#[derive(Clone)]
pub struct SqliteProvider {
    connection: Arc<Mutex<Connection>>,
}

impl SqliteProvider {
    pub fn new(path: Option<&Path>) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let path = resolve_db_path(path, "scoreboard-master.sqlite")?;
        tracing::info!("Using SQLite database at {}", path.display());

        let connection = Connection::open(&path)?;
        connection.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA busy_timeout = 5000;
             CREATE TABLE IF NOT EXISTS games (key TEXT PRIMARY KEY, value BLOB NOT NULL);
             CREATE TABLE IF NOT EXISTS devices (key TEXT PRIMARY KEY, value BLOB NOT NULL);",
        )?;

        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    pub(crate) fn upsert_many(
        &self,
        table: Table,
        entries: &[(String, Vec<u8>)],
    ) -> Result<(), String> {
        if entries.is_empty() {
            return Ok(());
        }
        let table_name = table.as_str();
        let mut conn = self
            .connection
            .lock()
            .map_err(|_| format!("SQLite connection lock poisoned while writing '{table_name}'"))?;
        let tx = conn
            .transaction()
            .map_err(|e| format!("failed to begin SQLite transaction for '{table_name}': {e}"))?;

        {
            let sql = format!("INSERT OR REPLACE INTO {table_name} (key, value) VALUES (?1, ?2)");
            let mut stmt = tx
                .prepare(&sql)
                .map_err(|e| format!("failed to prepare SQLite write for '{table_name}': {e}"))?;
            for (key, value) in entries {
                stmt.execute(params![key, value]).map_err(|e| {
                    format!("failed to write row into SQLite table '{table_name}': {e}")
                })?;
            }
        }

        tx.commit()
            .map_err(|e| format!("failed to commit SQLite transaction for '{table_name}': {e}"))
    }

    pub(crate) fn delete_many(&self, table: Table, keys: &[String]) -> Result<(), String> {
        if keys.is_empty() {
            return Ok(());
        }
        let table_name = table.as_str();
        let mut conn = self.connection.lock().map_err(|_| {
            format!("SQLite connection lock poisoned while deleting from '{table_name}'")
        })?;
        let tx = conn.transaction().map_err(|e| {
            format!("failed to begin SQLite delete transaction for '{table_name}': {e}")
        })?;

        {
            let sql = format!("DELETE FROM {table_name} WHERE key = ?1");
            let mut stmt = tx
                .prepare(&sql)
                .map_err(|e| format!("failed to prepare SQLite delete for '{table_name}': {e}"))?;
            for key in keys {
                stmt.execute(params![key]).map_err(|e| {
                    format!("failed to delete row from SQLite table '{table_name}': {e}")
                })?;
            }
        }

        tx.commit().map_err(|e| {
            format!("failed to commit SQLite delete transaction for '{table_name}': {e}")
        })
    }

    pub(crate) fn read_all(&self, table: Table) -> Result<Vec<(String, Vec<u8>)>, String> {
        let table_name = table.as_str();
        let conn = self
            .connection
            .lock()
            .map_err(|_| format!("SQLite connection lock poisoned while reading '{table_name}'"))?;
        let sql = format!("SELECT key, value FROM {table_name}");
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| format!("failed to prepare SQLite select for '{table_name}': {e}"))?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
            })
            .map_err(|e| format!("failed to query SQLite table '{table_name}': {e}"))?;

        rows.map(|row| {
            row.map_err(|e| format!("failed to decode SQLite row from '{table_name}': {e}"))
        })
        .collect()
    }
}
