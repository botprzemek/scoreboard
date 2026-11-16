use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use crate::adapters::repositories::{DeviceRepository, Repository};

use net::Device;
use tokio::sync::{broadcast, watch};
use tokio::task::JoinHandle;
use tokio::time::{self, MissedTickBehavior};
use tokio_util::sync::CancellationToken;

use super::service::{DeviceGameAssignment, DeviceService, DeviceStore, DirtyDevices};

impl DeviceService {
    #[must_use]
    pub fn watch(&self) -> watch::Receiver<Vec<Device>> {
        self.state.subscribe()
    }

    #[must_use]
    pub fn subscribe_disconnect(&self) -> broadcast::Receiver<String> {
        self.disconnect_tx.subscribe()
    }

    #[must_use]
    pub fn subscribe_game_assign(&self) -> broadcast::Receiver<DeviceGameAssignment> {
        self.game_assign_tx.subscribe()
    }
}

pub(super) fn spawn_db_flusher(
    device_repository: DeviceRepository,
    store: Arc<RwLock<DeviceStore>>,
    dirty_devices: Arc<Mutex<DirtyDevices>>,
    mut state_rx: watch::Receiver<Vec<Device>>,
    db_flush_interval: Duration,
    shutdown: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut database_interval = time::interval(db_flush_interval);
        database_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut dirty = false;

        loop {
            tokio::select! {
                changed = state_rx.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    dirty = true;
                }
                _ = database_interval.tick(), if dirty => {
                    if flush_devices(&device_repository, &store, &dirty_devices).await {
                        dirty = false;
                    }
                }
                () = shutdown.cancelled() => {
                    if dirty || state_rx.has_changed().unwrap_or(false) {
                        let _ = flush_devices(&device_repository, &store, &dirty_devices).await;
                    }
                    return;
                }
            }
        }
    })
}

async fn flush_devices(
    device_repository: &DeviceRepository,
    store: &RwLock<DeviceStore>,
    dirty_devices: &Mutex<DirtyDevices>,
) -> bool {
    let (upsert_serials, delete_serials): (Vec<String>, Vec<String>) = {
        let mut guard = dirty_devices
            .lock()
            .expect("dirty devices lock should not be poisoned");
        if guard.upserts.is_empty() && guard.deletes.is_empty() {
            return true;
        }
        (guard.upserts.drain().collect(), guard.deletes.drain().collect())
    };

    let upserts: Vec<Device> = {
        let store_guard = store
            .read()
            .expect("device store lock should not be poisoned");
        upsert_serials
            .iter()
            .filter_map(|serial| store_guard.devices.get(serial).copied())
            .collect()
    };

    if upserts.is_empty() && delete_serials.is_empty() {
        return true;
    }

    let repository = device_repository.clone();
    let deletes_for_task = delete_serials.clone();
    let ok = crate::services::retry_blocking("Device DB flush", move || {
        if !upserts.is_empty() {
            repository.upsert_many(&upserts)?;
        }
        if !deletes_for_task.is_empty() {
            repository.delete_many(&deletes_for_task)?;
        }
        Ok(())
    })
    .await;

    if !ok {
        let mut guard = dirty_devices
            .lock()
            .expect("dirty devices lock should not be poisoned");
        for serial in upsert_serials {
            if !guard.deletes.contains(&serial) {
                guard.upserts.insert(serial);
            }
        }
        for serial in delete_serials {
            if !guard.upserts.contains(&serial) {
                guard.deletes.insert(serial);
            }
        }
    }

    ok
}
