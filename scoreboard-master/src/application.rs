use std::net::SocketAddr;

use crate::adapters::data::DataProvider;
use crate::adapters::net::{WebDriver, WorkerDriver};
use crate::adapters::repositories::Registry;
use crate::config::Config;
use crate::error::AppError;
use crate::services::{DeviceService, GameService};

use tokio::net::{TcpListener, UdpSocket};
use tokio::signal;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

pub struct Application {
    config: Config,
}

impl Default for Application {
    fn default() -> Self {
        Self::from_config(Config::default())
    }
}

impl Application {
    pub fn new() -> Result<Self, AppError> {
        Ok(Self {
            config: Config::from_env()?,
        })
    }

    #[must_use]
    pub fn from_config(config: Config) -> Self {
        Self { config }
    }

    #[must_use]
    pub fn with_config(mut self, config: Config) -> Self {
        self.config = config;
        self
    }

    async fn wait_for_ctrl_c() -> Result<(), AppError> {
        signal::ctrl_c().await?;
        Ok(())
    }

    async fn wait_for_terminate() -> Result<(), AppError> {
        #[cfg(not(unix))]
        std::future::pending::<()>().await;

        #[cfg(unix)]
        {
            signal::unix::signal(signal::unix::SignalKind::terminate())?
                .recv()
                .await;
        }
        Ok(())
    }

    async fn wait_for_shutdown() -> Result<(), AppError> {
        tokio::select! {
            result = Self::wait_for_ctrl_c() => {
                result?;
                tracing::info!("Gracefully exiting (CTRL+C)");
            }
            result = Self::wait_for_terminate() => {
                result?;
                tracing::info!("Gracefully exiting (SIGTERM)");
            }
        }
        Ok(())
    }

    async fn bind_http(address: Option<&str>) -> Result<Option<TcpListener>, AppError> {
        let Some(address) = address else {
            tracing::info!("HTTP listener is not enabled... omitting");
            return Ok(None);
        };

        tracing::info!("Listening on http://{address}");
        Ok(Some(TcpListener::bind(address).await?))
    }

    fn bind_udp(address: Option<&str>) -> Result<Option<UdpSocket>, AppError> {
        let Some(address) = address else {
            tracing::info!("UDP listener is not enabled... omitting");
            return Ok(None);
        };

        tracing::info!("Listening on udp://{address}");

        let addr: SocketAddr = address.parse()?;
        let socket2_sock = socket2::Socket::new(
            if addr.is_ipv6() {
                socket2::Domain::IPV6
            } else {
                socket2::Domain::IPV4
            },
            socket2::Type::DGRAM,
            Some(socket2::Protocol::UDP),
        )?;

        if let Err(error) = socket2_sock.set_recv_buffer_size(8 * 1024 * 1024) {
            tracing::warn!("Failed to set UDP receive buffer size: {error}");
        }
        if let Err(error) = socket2_sock.set_send_buffer_size(8 * 1024 * 1024) {
            tracing::warn!("Failed to set UDP send buffer size: {error}");
        }

        socket2_sock.set_nonblocking(true)?;
        socket2_sock.bind(&addr.into())?;

        let std_socket: std::net::UdpSocket = socket2_sock.into();

        Ok(Some(UdpSocket::from_std(std_socket)?))
    }

    pub async fn run(self) -> Result<(), AppError> {
        if self.config.http_address.is_none() && self.config.udp_address.is_none() {
            return Err("At least one of HTTP or UDP listener must be enabled".into());
        }

        let net_shutdown = CancellationToken::new();
        let actors_shutdown = CancellationToken::new();
        let db_shutdown = CancellationToken::new();
        let mut net_handles: Vec<JoinHandle<()>> = Vec::new();
        let mut service_handles: Vec<JoinHandle<()>> = Vec::new();

        let init_config = self.config.clone();
        let init_actors_shutdown = actors_shutdown.clone();
        let init_db_shutdown = db_shutdown.clone();
        let (game, devices, init_service_handles) =
            tokio::task::spawn_blocking(move || -> Result<_, AppError> {
                let database = DataProvider::from_config(&init_config)?;
                let registry = Registry::new(database);
                let (game, game_handles) = GameService::new(
                    registry.game().clone(),
                    init_config.database_flush_interval,
                    init_actors_shutdown,
                    init_db_shutdown.clone(),
                )?;
                let (devices, device_handle) = DeviceService::new(
                    registry.devices().clone(),
                    init_config.database_flush_interval,
                    init_db_shutdown,
                )?;
                let mut handles = game_handles;
                handles.push(device_handle);
                Ok((game, devices, handles))
            })
            .await??;
        service_handles.extend(init_service_handles);

        if let Some(listener) = Self::bind_http(self.config.http_address.as_deref()).await? {
            net_handles.push(
                WebDriver::new(listener, game.clone(), devices.clone())
                    .with_cors_origins(self.config.cors_origins)
                    .with_shutdown(net_shutdown.clone())
                    .spawn(),
            );
        }

        if let Some(socket) = Self::bind_udp(self.config.udp_address.as_deref())? {
            net_handles.push(
                WorkerDriver::new(socket, game.clone(), devices)
                    .with_worker_timeout(self.config.worker_timeout)
                    .with_shutdown(net_shutdown.clone())
                    .spawn(),
            );
        }

        Self::wait_for_shutdown().await?;

        net_shutdown.cancel();
        for handle in net_handles {
            if let Err(error) = handle.await {
                tracing::error!("Network task terminated with error: {error}");
            }
        }

        actors_shutdown.cancel();
        game.shutdown().await;

        db_shutdown.cancel();
        for handle in service_handles {
            if let Err(error) = handle.await {
                tracing::error!("Background task terminated with error: {error}");
            }
        }

        Ok(())
    }
}
