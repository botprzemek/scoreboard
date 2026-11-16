use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use crate::adapters::data::Table;

use redb::{Builder, Database, ReadableDatabase, ReadableTable, TableDefinition};

use super::provider::resolve_db_path;

const DATABASE_CACHE_SIZE: usize = 4 * 1024 * 1024;
const GAME_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new(Table::Games.as_str());
const DEVICES_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new(Table::Devices.as_str());

#[derive(Clone)]
pub struct ReDBProvider {
    connection: Arc<Database>,
}

impl ReDBProvider {
    pub fn new(path: Option<&Path>) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let database_path = resolve_db_path(path, "scoreboard-master.redb")?;
        tracing::info!("Using ReDB database at {}", database_path.display());

        let connection = Builder::new()
            .set_cache_size(DATABASE_CACHE_SIZE)
            .create(&database_path)?;

        {
            let write_transaction = connection.begin_write()?;
            write_transaction.open_table(GAME_TABLE)?;
            write_transaction.open_table(DEVICES_TABLE)?;
            write_transaction.commit()?;
        }

        Ok(Self {
            connection: Arc::new(connection),
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
        let table_def = table_definition(table);
        let transaction = self
            .connection
            .begin_write()
            .map_err(|e| format!("failed to begin ReDB write transaction for '{table}': {e}"))?;
        {
            let mut table_handle = transaction
                .open_table(table_def)
                .map_err(|e| format!("failed to open ReDB table '{table}': {e}"))?;
            for (key, value) in entries {
                table_handle
                    .insert(key.as_str(), value.as_slice())
                    .map_err(|e| format!("failed to write row into ReDB table '{table}': {e}"))?;
            }
        }
        transaction
            .commit()
            .map_err(|e| format!("failed to commit ReDB write transaction for '{table}': {e}"))
    }

    pub(crate) fn delete_many(&self, table: Table, keys: &[String]) -> Result<(), String> {
        if keys.is_empty() {
            return Ok(());
        }
        let table_def = table_definition(table);
        let transaction = self
            .connection
            .begin_write()
            .map_err(|e| format!("failed to begin ReDB delete transaction for '{table}': {e}"))?;
        {
            let mut table_handle = transaction
                .open_table(table_def)
                .map_err(|e| format!("failed to open ReDB table '{table}': {e}"))?;
            for key in keys {
                table_handle
                    .remove(key.as_str())
                    .map_err(|e| format!("failed to delete row from ReDB table '{table}': {e}"))?;
            }
        }
        transaction
            .commit()
            .map_err(|e| format!("failed to commit ReDB delete transaction for '{table}': {e}"))
    }

    pub(crate) fn read_all(&self, table: Table) -> Result<Vec<(String, Vec<u8>)>, String> {
        let table_def = table_definition(table);
        let transaction = self
            .connection
            .begin_read()
            .map_err(|e| format!("failed to begin ReDB read transaction for '{table}': {e}"))?;
        let table_handle = transaction
            .open_table(table_def)
            .map_err(|e| format!("failed to open ReDB table '{table}': {e}"))?;
        let iter = table_handle
            .iter()
            .map_err(|e| format!("failed to iterate ReDB table '{table}': {e}"))?;

        iter.map(|entry| {
            let (k, v) =
                entry.map_err(|e| format!("failed to read row from ReDB table '{table}': {e}"))?;
            Ok((k.value().to_string(), v.value().to_vec()))
        })
        .collect()
    }
}

fn table_definition(table: Table) -> TableDefinition<'static, &'static str, &'static [u8]> {
    match table {
        Table::Games => GAME_TABLE,
        Table::Devices => DEVICES_TABLE,
    }
}
