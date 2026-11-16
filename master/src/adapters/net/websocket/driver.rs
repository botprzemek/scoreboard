use super::handler::WebsocketHandler;
use crate::services::{DeviceService, GameService};

use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

pub struct WebDriver {
    listener: TcpListener,
    game: GameService,
    devices: DeviceService,
    cors_origins: Vec<String>,
    shutdown: CancellationToken,
}

impl WebDriver {
    #[must_use]
    pub fn new(listener: TcpListener, game: GameService, devices: DeviceService) -> Self {
        Self {
            listener,
            game,
            devices,
            cors_origins: Vec::new(),
            shutdown: CancellationToken::new(),
        }
    }

    #[must_use]
    pub fn with_cors_origins(mut self, cors_origins: Vec<String>) -> Self {
        self.cors_origins = cors_origins;
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
        WebsocketHandler::run(
            self.listener,
            self.game,
            self.devices,
            &self.cors_origins,
            self.shutdown,
        )
        .await;
    }
}
