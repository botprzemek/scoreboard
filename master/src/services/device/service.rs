use super::sync;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use crate::adapters::repositories::{DeviceRepository, Repository};

use config::{ConfigError, DeviceConfig};
use net::{Device, DeviceType};
use tokio::sync::{broadcast, watch};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceGameAssignment {
    pub serial_number: String,
    pub game_id: u16,
}

#[derive(Clone)]
pub struct DeviceService {
    #[allow(dead_code)]
    device_repository: DeviceRepository,
    pub(super) state: watch::Sender<Vec<Device>>,
    assigned_games: Arc<RwLock<HashMap<String, u16>>>,
    pub(super) broadcast: broadcast::Sender<Vec<Device>>,
    pub(super) disconnect_tx: broadcast::Sender<String>,
    pub(super) game_assign_tx: broadcast::Sender<DeviceGameAssignment>,
}

impl DeviceService {
    #[must_use]
    pub fn new(device_repository: DeviceRepository, db_flush_interval: Duration) -> Self {
        let initial_devices = device_repository.select();
        let assigned_games = Arc::new(RwLock::new(Self::collect_assignments(&initial_devices)));
        let (state, state_rx) = watch::channel(initial_devices);
        let (broadcast, _broadcast_rx) = broadcast::channel(64);
        let (disconnect_tx, _disconnect_rx) = broadcast::channel(64);
        let (game_assign_tx, _game_assign_rx) = broadcast::channel(64);

        sync::spawn_db_flusher(device_repository.clone(), state_rx, db_flush_interval);

        Self {
            device_repository,
            state,
            assigned_games,
            broadcast,
            disconnect_tx,
            game_assign_tx,
        }
    }

    #[must_use]
    pub fn all(&self) -> Vec<Device> {
        self.state.borrow().clone()
    }

    #[must_use]
    pub fn game_id_by_serial(&self, serial_number: &str) -> Option<u16> {
        if let Some(&game_id) = self
            .assigned_games
            .read()
            .expect("assigned_games lock should not be poisoned")
            .get(serial_number)
            && game_id != 0
        {
            return Some(game_id);
        }

        self.state
            .borrow()
            .iter()
            .find(|device| device.serial_number() == Ok(serial_number))
            .map(Device::game_id)
    }

    pub fn assign_game_by_serial(&self, serial_number: &str, game_id: u16) -> Option<Device> {
        self.assigned_games
            .write()
            .expect("assigned_games lock should not be poisoned")
            .insert(serial_number.to_string(), game_id);

        let mut updated_device = None;

        self.state.send_modify(|devices| {
            if let Some(existing) = devices
                .iter_mut()
                .find(|device| device.serial_number() == Ok(serial_number))
            {
                existing.set_game_id(game_id);
                updated_device = Some(*existing);
            } else if let Ok(stub) =
                Device::try_new_with_game_id(DeviceType::Worker, game_id, serial_number, "pending")
            {
                devices.push(stub);
                updated_device = Some(stub);
            }
        });

        if updated_device.is_some() {
            let _ = self.game_assign_tx.send(DeviceGameAssignment {
                serial_number: serial_number.to_string(),
                game_id,
            });
            self.publish_snapshot();
        }

        updated_device
    }

    pub fn validate_and_register(&self, device: Device) -> Result<Device, ConfigError> {
        device.validate()?;

        let serial_number = device.serial_number()?;
        let device_id = device.device_id()?;

        if serial_number.is_empty() || device_id.is_empty() {
            return Err(ConfigError::Serialization);
        }

        {
            let mut assigned = self
                .assigned_games
                .write()
                .expect("assigned_games lock should not be poisoned");
            let game_id = device.game_id();
            if game_id != 0 {
                assigned.insert(serial_number.to_string(), game_id);
            } else {
                assigned.remove(serial_number);
            }
        }

        let modified = self.state.send_if_modified(|devices| {
            if let Some(existing) = devices
                .iter_mut()
                .find(|entry| entry.serial_number() == Ok(serial_number))
            {
                if *existing != device {
                    *existing = device;
                    true
                } else {
                    false
                }
            } else {
                devices.push(device);
                true
            }
        });

        if modified {
            self.publish_snapshot();
        }

        Ok(device)
    }

    pub fn unregister(&self, device: &Device) {
        if let Ok(serial_number) = device.serial_number() {
            self.remove_by_serial(serial_number);
        }
    }

    pub fn disconnect_by_serial(&self, serial_number: &str) {
        self.remove_by_serial(serial_number);
        let _ = self.disconnect_tx.send(serial_number.to_string());
    }

    fn collect_assignments(devices: &[Device]) -> HashMap<String, u16> {
        devices
            .iter()
            .filter_map(|device| {
                let serial = device.serial_number().ok()?;
                let game_id = device.game_id();
                if game_id == 0 {
                    return None;
                }
                Some((serial.to_string(), game_id))
            })
            .collect()
    }

    fn remove_by_serial(&self, serial_number: &str) {
        let modified = self.state.send_if_modified(|devices| {
            let before = devices.len();
            devices.retain(|device| device.serial_number() != Ok(serial_number));
            devices.len() != before
        });
        if modified {
            self.publish_snapshot();
        }
    }
}
