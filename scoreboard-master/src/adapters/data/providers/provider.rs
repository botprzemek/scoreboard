use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use crate::adapters::data::Table;
use crate::config::{DatabaseBackend, DatabaseConfig};

use super::redb::ReDBProvider;
use super::sqlite::SqliteProvider;

#[derive(Clone)]
pub enum DataProvider {
    ReDB(ReDBProvider),
    Sqlite(SqliteProvider),
}

macro_rules! dispatch {
    ($self:expr, $provider:ident => $call:expr) => {
        match $self {
            Self::ReDB($provider) => $call,
            Self::Sqlite($provider) => $call,
        }
    };
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
    pub fn from_config(config: &impl DatabaseConfig) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let path = config.database_path();

        match Self::resolve_backend(config) {
            DatabaseBackend::Sqlite => Ok(Self::Sqlite(SqliteProvider::new(path)?)),
            DatabaseBackend::ReDB => Ok(Self::ReDB(ReDBProvider::new(path)?)),
        }
    }

    pub fn read_all(&self, table: Table) -> Result<Vec<(String, Vec<u8>)>, String> {
        dispatch!(self, provider => provider.read_all(table))
    }

    pub fn upsert_many(&self, table: Table, entries: &[(String, Vec<u8>)]) -> Result<(), String> {
        if entries.is_empty() {
            return Ok(());
        }

        dispatch!(self, provider => provider.upsert_many(table, entries))
    }

    pub fn delete_many(&self, table: Table, keys: &[String]) -> Result<(), String> {
        if keys.is_empty() {
            return Ok(());
        }

        dispatch!(self, provider => provider.delete_many(table, keys))
    }

    #[must_use]
    fn resolve_backend(config: &impl DatabaseConfig) -> DatabaseBackend {
        match config
            .database_path()
            .and_then(Path::extension)
            .and_then(|ext| ext.to_str())
        {
            Some("sqlite" | "db") => DatabaseBackend::Sqlite,
            Some("redb") => DatabaseBackend::ReDB,
            _ => config.database_backend(),
        }
    }
}

pub(super) fn resolve_db_path(
    path: Option<&Path>,
    default_filename: &str,
) -> std::io::Result<PathBuf> {
    let resolved = path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| default_db_path(default_filename));

    if let Some(parent) = resolved.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }

    Ok(resolved)
}

fn default_db_path(filename: &str) -> PathBuf {
    if let Ok(path_str) = env::var("MASTER_DB_PATH") {
        return PathBuf::from(path_str);
    }

    let master_local = Path::new("master").join(filename);
    if master_local.exists() {
        return master_local;
    }
    if Path::new(filename).exists() {
        return PathBuf::from(filename);
    }

    if let Ok(home) = env::var("HOME") {
        return PathBuf::from(home)
            .join(".local/share/scoreboard-master")
            .join(filename);
    }

    PathBuf::from(filename)
}
