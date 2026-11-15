use std::time::Duration;

use crate::adapters::repositories::{DeviceRepository, Repository};
use crate::services::Observable;

use net::Device;
use tokio::sync::{broadcast, watch};
use tokio::task;
use tokio::time::{self, MissedTickBehavior};

use super::service::{DeviceGameAssignment, DeviceService};

impl Observable<Vec<Device>> for DeviceService {
    fn subscribe(&self) -> broadcast::Receiver<Vec<Device>> {
        self.broadcast.subscribe()
    }

    fn watch(&self) -> watch::Receiver<Vec<Device>> {
        self.state.subscribe()
    }
}

impl DeviceService {
    #[must_use]
    pub fn subscribe_disconnect(&self) -> broadcast::Receiver<String> {
        self.disconnect_tx.subscribe()
    }

    #[must_use]
    pub fn subscribe_game_assign(&self) -> broadcast::Receiver<DeviceGameAssignment> {
        self.game_assign_tx.subscribe()
    }

    pub(super) fn publish_snapshot(&self) {
        if self.broadcast.receiver_count() > 0 {
            let _ = self.broadcast.send(self.all());
        }
    }
}

pub(super) fn spawn_db_flusher(
    device_repository: DeviceRepository,
    mut state_rx: watch::Receiver<Vec<Device>>,
    db_flush_interval: Duration,
) {
    tokio::spawn(async move {
        let mut database_interval = time::interval(db_flush_interval);
        database_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            if state_rx.changed().await.is_err() {
                break;
            }
            database_interval.tick().await;
            let snapshot = state_rx.borrow_and_update().clone();
            let repository = device_repository.clone();
            let _ = task::spawn_blocking(move || {
                repository.upsert(&snapshot);
            })
            .await;
        }
    });
}
