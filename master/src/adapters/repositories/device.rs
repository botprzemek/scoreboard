use crate::adapters::data::{DEVICES, DataProvider};
use crate::adapters::repositories::Repository;

use config::DeviceConfig;
use net::Device;

#[derive(Clone)]
pub struct DeviceRepository {
    database: DataProvider,
}

impl Repository<[Device]> for DeviceRepository {
    type Output = Vec<Device>;

    fn select(&self) -> Vec<Device> {
        self.database
            .read_all(DEVICES)
            .into_iter()
            .filter_map(Self::decode_entry)
            .collect()
    }

    fn upsert(&self, entities: &[Device]) {
        let entries = entities
            .iter()
            .filter_map(Self::to_entry)
            .collect::<Vec<(String, Vec<u8>)>>();

        self.database.replace_all(DEVICES, &entries);
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

    fn decode_entry((_key, value): (String, Vec<u8>)) -> Option<Device> {
        Device::from_bytes(&value).ok()
    }
}
