use super::sync;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use crate::adapters::repositories::{DeviceRepository, Repository};

use scoreboard_config::{ConfigError, DeviceConfig};
use scoreboard_net::{Device, DeviceType};
use tokio::sync::{broadcast, watch};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceGameAssignment {
    pub serial_number: String,
    pub game_id: u64,
}

pub(super) struct DeviceStore {
    pub(super) devices: HashMap<String, Device>,
    assigned_games: HashMap<String, u64>,
}

#[derive(Default)]
pub(super) struct DirtyDevices {
    pub(super) upserts: HashSet<String>,
    pub(super) deletes: HashSet<String>,
}

impl DirtyDevices {
    pub(super) fn mark_upserted(&mut self, serial: &str) {
        self.deletes.remove(serial);
        self.upserts.insert(serial.to_string());
    }

    pub(super) fn mark_deleted(&mut self, serial: &str) {
        self.upserts.remove(serial);
        self.deletes.insert(serial.to_string());
    }
}

impl DeviceStore {
    fn from_initial(initial_devices: &[Device]) -> Self {
        let mut devices = HashMap::new();
        let mut assigned_games = HashMap::new();

        for &device in initial_devices {
            let Ok(serial) = device.serial_number() else {
                continue;
            };
            let key = serial.to_string();
            if device.game_id() != 0 {
                assigned_games.insert(key.clone(), device.game_id());
            }
            devices.insert(key, device);
        }

        Self {
            devices,
            assigned_games,
        }
    }

    fn snapshot(&self) -> Vec<Device> {
        self.devices.values().copied().collect()
    }
}

#[derive(Clone)]
pub struct DeviceService {
    store: Arc<RwLock<DeviceStore>>,
    dirty: Arc<Mutex<DirtyDevices>>,
    pub(super) state: watch::Sender<Vec<Device>>,
    pub(super) disconnect_tx: broadcast::Sender<String>,
    pub(super) game_assign_tx: broadcast::Sender<DeviceGameAssignment>,
}

impl DeviceService {
    pub fn new(
        device_repository: DeviceRepository,
        db_flush_interval: Duration,
        shutdown: CancellationToken,
    ) -> Result<(Self, JoinHandle<()>), String> {
        let initial_devices = device_repository.select()?;
        let store = Arc::new(RwLock::new(DeviceStore::from_initial(&initial_devices)));
        let dirty = Arc::new(Mutex::new(DirtyDevices::default()));
        let (state, state_rx) = watch::channel(initial_devices);
        let (disconnect_tx, _disconnect_rx) = broadcast::channel(64);
        let (game_assign_tx, _game_assign_rx) = broadcast::channel(64);

        let flusher_handle = sync::spawn_db_flusher(
            device_repository,
            Arc::clone(&store),
            Arc::clone(&dirty),
            state_rx,
            db_flush_interval,
            shutdown,
        );

        let service = Self {
            store,
            dirty,
            state,
            disconnect_tx,
            game_assign_tx,
        };

        Ok((service, flusher_handle))
    }

    #[must_use]
    pub fn game_id_by_serial(&self, serial_number: &str) -> Option<u64> {
        let store = self
            .store
            .read()
            .expect("device store lock should not be poisoned");
        if let Some(&game_id) = store.assigned_games.get(serial_number)
            && game_id != 0
        {
            return Some(game_id);
        }

        store.devices.get(serial_number).map(Device::game_id)
    }

    pub fn assign_game_by_serial(&self, serial_number: &str, game_id: u64) -> Option<Device> {
        let (updated_device, snapshot) = {
            let mut store = self
                .store
                .write()
                .expect("device store lock should not be poisoned");
            if game_id != 0 {
                store
                    .assigned_games
                    .insert(serial_number.to_string(), game_id);
            } else {
                store.assigned_games.remove(serial_number);
            }

            let updated_device = if let Some(existing) = store.devices.get_mut(serial_number) {
                existing.set_game_id(game_id);
                Some(*existing)
            } else if let Ok(stub) =
                Device::try_new_with_game_id(DeviceType::Worker, game_id, serial_number, "pending")
            {
                store.devices.insert(serial_number.to_string(), stub);
                Some(stub)
            } else {
                None
            };

            let snapshot = updated_device.map(|_| store.snapshot());
            (updated_device, snapshot)
        };

        if let Some(snapshot) = snapshot {
            self.dirty
                .lock()
                .expect("dirty devices lock should not be poisoned")
                .mark_upserted(serial_number);
            let _ = self.state.send(snapshot);
            let _ = self.game_assign_tx.send(DeviceGameAssignment {
                serial_number: serial_number.to_string(),
                game_id,
            });
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

        let snapshot = {
            let mut store = self
                .store
                .write()
                .expect("device store lock should not be poisoned");
            let game_id = device.game_id();
            if game_id != 0 {
                store
                    .assigned_games
                    .insert(serial_number.to_string(), game_id);
            } else {
                store.assigned_games.remove(serial_number);
            }

            let changed = store
                .devices
                .get(serial_number)
                .is_none_or(|existing| *existing != device);
            if changed {
                store.devices.insert(serial_number.to_string(), device);
                Some(store.snapshot())
            } else {
                None
            }
        };

        if let Some(snapshot) = snapshot {
            self.dirty
                .lock()
                .expect("dirty devices lock should not be poisoned")
                .mark_upserted(serial_number);
            let _ = self.state.send(snapshot);
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

    fn remove_by_serial(&self, serial_number: &str) {
        let snapshot = {
            let mut store = self
                .store
                .write()
                .expect("device store lock should not be poisoned");
            let removed = store.devices.remove(serial_number).is_some();
            removed.then(|| store.snapshot())
        };

        if let Some(snapshot) = snapshot {
            self.dirty
                .lock()
                .expect("dirty devices lock should not be poisoned")
                .mark_deleted(serial_number);
            let _ = self.state.send(snapshot);
        }
    }
}
