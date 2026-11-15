use std::sync::Arc;

use super::handler::WebsocketHandler;
use crate::services::Services;

use tokio::net::TcpListener;

pub struct WebDriver {
    listener: TcpListener,
    cors_origins: Vec<String>,
}

impl WebDriver {
    #[must_use]
    pub fn new(listener: TcpListener) -> Self {
        Self {
            listener,
            cors_origins: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_cors_origins(mut self, cors_origins: Vec<String>) -> Self {
        self.cors_origins = cors_origins;
        self
    }

    pub fn spawn(self, services: Arc<Services>) {
        tokio::spawn(self.run(services));
    }

    async fn run(self, services: Arc<Services>) {
        axum::serve(
            self.listener,
            WebsocketHandler::router(services, &self.cors_origins),
        )
        .await
        .expect("HTTP server failed");
    }
}
