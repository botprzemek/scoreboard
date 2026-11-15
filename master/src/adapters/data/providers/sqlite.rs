use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rusqlite::{Connection, params};

#[derive(Clone)]
pub struct SqliteProvider {
    connection: Arc<Mutex<Connection>>,
}

impl Default for SqliteProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SqliteProvider {
    #[must_use]
    pub fn default_path() -> PathBuf {
        if let Ok(path_str) = env::var("MASTER_DB_PATH") {
            return PathBuf::from(path_str);
        }

        if Path::new("master/scoreboard-master.sqlite").exists() {
            return PathBuf::from("master/scoreboard-master.sqlite");
        }
        if Path::new("scoreboard-master.sqlite").exists() {
            return PathBuf::from("scoreboard-master.sqlite");
        }

        if let Ok(home) = env::var("HOME") {
            let data_dir = PathBuf::from(home).join(".local/share/scoreboard-master");
            let _ = fs::create_dir_all(&data_dir);
            return data_dir.join("scoreboard-master.sqlite");
        }

        PathBuf::from("scoreboard-master.sqlite")
    }

    #[must_use]
    pub fn new() -> Self {
        Self::open(Self::default_path())
    }

    #[must_use]
    pub fn open<P: AsRef<Path>>(path: P) -> Self {
        let path = path.as_ref();
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            let _ = fs::create_dir_all(parent);
        }

        let connection = Connection::open(path).expect("failed to open SQLite database");
        connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS games (key TEXT PRIMARY KEY, value BLOB NOT NULL);
                 CREATE TABLE IF NOT EXISTS devices (key TEXT PRIMARY KEY, value BLOB NOT NULL);",
            )
            .expect("failed to initialize SQLite tables");

        Self {
            connection: Arc::new(Mutex::new(connection)),
        }
    }

    #[must_use]
    pub fn read_all(&self, table: &str) -> Vec<(String, Vec<u8>)> {
        let table = validated_table(table);
        let conn = self
            .connection
            .lock()
            .expect("SQLite connection lock should not be poisoned");
        let sql = format!("SELECT key, value FROM {table}");
        let mut stmt = conn.prepare(&sql).expect("failed to prepare SQLite select");
        let rows = stmt
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?)))
            .expect("failed to query SQLite table");

        rows.filter_map(Result::ok).collect()
    }

    pub fn replace_all(&self, table: &str, entries: &[(String, Vec<u8>)]) {
        let table = validated_table(table);
        let mut conn = self
            .connection
            .lock()
            .expect("SQLite connection lock should not be poisoned");
        let tx = conn
            .transaction()
            .expect("failed to begin SQLite transaction");

        tx.execute(&format!("DELETE FROM {table}"), [])
            .expect("failed to clear SQLite table");

        {
            let sql = format!("INSERT INTO {table} (key, value) VALUES (?1, ?2)");
            let mut stmt = tx.prepare(&sql).expect("failed to prepare SQLite insert");
            for (key, value) in entries {
                stmt.execute(params![key, value])
                    .expect("failed to insert row into SQLite table");
            }
        }

        tx.commit().expect("failed to commit SQLite transaction");
    }
}

fn validated_table(table: &str) -> &'static str {
    match table {
        "games" => "games",
        "devices" => "devices",
        _ => panic!("unsupported SQLite table: {table}"),
    }
}
