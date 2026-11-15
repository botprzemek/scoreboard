use crate::config::{Config, DatabaseBackend};

use ::redb::{ReadableDatabase, ReadableTable};

use super::redb::{DEVICES_TABLE, GAME_TABLE, ReDBProvider};
use super::sqlite::SqliteProvider;

#[derive(Clone)]
pub enum DataProvider {
    ReDB(ReDBProvider),
    Sqlite(SqliteProvider),
}

impl From<ReDBProvider> for DataProvider {
    fn from(provider: ReDBProvider) -> Self {
        Self::ReDB(provider)
    }
}

impl From<SqliteProvider> for DataProvider {
    fn from(provider: SqliteProvider) -> Self {
        Self::Sqlite(provider)
    }
}

impl DataProvider {
    #[must_use]
    pub fn from_path(database_path: Option<&std::path::Path>) -> Self {
        Self::from_config(database_path, Config::from_env().database_backend)
    }

    #[must_use]
    pub fn from_config(
        database_path: Option<&std::path::Path>,
        backend: DatabaseBackend,
    ) -> Self {
        match database_path {
            Some(path) => match path.extension().and_then(|ext| ext.to_str()) {
                Some("sqlite" | "db") => Self::open_sqlite(path),
                Some("redb") => Self::open_redb(path),
                _ => match backend {
                    DatabaseBackend::Sqlite => Self::open_sqlite(path),
                    DatabaseBackend::ReDB => Self::open_redb(path),
                },
            },
            None => match backend {
                DatabaseBackend::Sqlite => Self::open_sqlite(&SqliteProvider::default_path()),
                DatabaseBackend::ReDB => Self::open_redb(&ReDBProvider::default_path()),
            },
        }
    }

    fn open_sqlite(path: &std::path::Path) -> Self {
        println!("INFO: Using SQLite database at {}", path.display());
        Self::Sqlite(SqliteProvider::open(path))
    }

    fn open_redb(path: &std::path::Path) -> Self {
        println!("INFO: Using ReDB database at {}", path.display());
        Self::ReDB(ReDBProvider::open(path))
    }

    #[must_use]
    pub fn read_all(&self, table: &str) -> Vec<(String, Vec<u8>)> {
        match self {
            Self::ReDB(provider) => {
                let table_def = match table {
                    "games" => GAME_TABLE,
                    "devices" => DEVICES_TABLE,
                    _ => panic!("unsupported ReDB table: {table}"),
                };
                let transaction = provider
                    .connection()
                    .begin_read()
                    .expect("failed to begin ReDB read transaction");
                let table = transaction
                    .open_table(table_def)
                    .expect("failed to open ReDB table");
                table
                    .iter()
                    .expect("failed to iterate over ReDB table")
                    .flatten()
                    .map(|(k, v)| (k.value().to_string(), v.value().to_vec()))
                    .collect()
            }
            Self::Sqlite(provider) => provider.read_all(table),
        }
    }

    pub fn replace_all(&self, table: &str, entries: &[(String, Vec<u8>)]) {
        match self {
            Self::ReDB(provider) => {
                let table_def = match table {
                    "games" => GAME_TABLE,
                    "devices" => DEVICES_TABLE,
                    _ => panic!("unsupported ReDB table: {table}"),
                };
                let transaction = provider
                    .connection()
                    .begin_write()
                    .expect("failed to begin ReDB write transaction");
                {
                    let mut table = transaction
                        .open_table(table_def)
                        .expect("failed to open ReDB table");
                    let keys: Vec<String> = table
                        .iter()
                        .expect("failed to iterate over ReDB table")
                        .flatten()
                        .map(|(k, _)| k.value().to_string())
                        .collect();
                    for key in keys {
                        let _ = table.remove(key.as_str());
                    }
                    for (key, value) in entries {
                        table
                            .insert(key.as_str(), value.as_slice())
                            .expect("failed to insert row into ReDB table");
                    }
                }
                transaction
                    .commit()
                    .expect("failed to commit ReDB write transaction");
            }
            Self::Sqlite(provider) => provider.replace_all(table, entries),
        }
    }
}
