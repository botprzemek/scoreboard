use std::sync::Arc;
use std::time::Duration;

use crate::config::Config;
use crate::services::{DeviceService, GameService};

use tokio::net::UdpSocket;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::handler::WorkerHandler;

pub struct WorkerDriver {
    listener: Arc<UdpSocket>,
    game: GameService,
    devices: DeviceService,
    worker_timeout: Duration,
    shutdown: CancellationToken,
}

impl WorkerDriver {
    #[must_use]
    pub fn new(listener: UdpSocket, game: GameService, devices: DeviceService) -> Self {
        Self {
            listener: Arc::new(listener),
            game,
            devices,
            worker_timeout: Config::default().worker_timeout,
            shutdown: CancellationToken::new(),
        }
    }

    #[must_use]
    pub fn with_worker_timeout(mut self, worker_timeout: Duration) -> Self {
        self.worker_timeout = worker_timeout;
        self
    }

    #[must_use]
    pub fn with_shutdown(mut self, shutdown: CancellationToken) -> Self {
        self.shutdown = shutdown;
        self
    }

    #[must_use]
    pub fn spawn(self) -> JoinHandle<()> {
        tokio::spawn(self.run())
    }

    pub async fn run(self) {
        WorkerHandler::run(
            self.listener,
            self.worker_timeout,
            self.game,
            self.devices,
            self.shutdown,
        )
        .await;
    }
}
