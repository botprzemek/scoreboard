use crate::adapters::data::{DataProvider, Table};
use crate::adapters::repositories::Repository;

use scoreboard_config::DeviceConfig;
use scoreboard_net::Device;

#[derive(Clone)]
pub struct DeviceRepository {
    database: DataProvider,
}

impl Repository<Device> for DeviceRepository {
    type Output = Vec<Device>;

    fn select(&self) -> Result<Vec<Device>, String> {
        let devices = self
            .database
            .read_all(Table::Devices)?
            .into_iter()
            .filter_map(Self::decode_entry)
            .collect();
        Ok(devices)
    }

    fn upsert_many(&self, entities: &[Device]) -> Result<(), String> {
        if entities.is_empty() {
            return Ok(());
        }
        let entries = entities
            .iter()
            .filter_map(Self::to_entry)
            .collect::<Vec<(String, Vec<u8>)>>();

        self.database.upsert_many(Table::Devices, &entries)
    }

    fn delete_many(&self, keys: &[String]) -> Result<(), String> {
        self.database.delete_many(Table::Devices, keys)
    }
}

impl DeviceRepository {
    #[must_use]
    pub fn new(database: impl Into<DataProvider>) -> Self {
        Self {
            database: database.into(),
        }
    }

    fn to_entry(device: &Device) -> Option<(String, Vec<u8>)> {
        let serial = device.serial_number().ok()?.to_string();

        Some((serial, device.to_bytes().to_vec()))
    }

    fn decode_entry((key, value): (String, Vec<u8>)) -> Option<Device> {
        match Device::from_bytes(&value) {
            Ok(device) => Some(device),
            Err(error) => {
                tracing::warn!("Skipping invalid device record for key '{key}': {error}");
                None
            }
        }
    }
}
