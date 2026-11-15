use std::sync::Arc;

use crate::adapters::data::DataProvider;
use crate::adapters::net::{WebDriver, WorkerDriver};
use crate::adapters::repositories::Registry;
use crate::config::Config;
use crate::services::Services;

use tokio::net::{TcpListener, UdpSocket};
use tokio::signal;

pub struct Application {
    config: Config,
}

pub struct ApplicationBuilder {
    config: Config,
}

impl Default for ApplicationBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl Application {
    async fn wait_for_ctrl_c() {
        signal::ctrl_c()
            .await
            .expect("ERROR: Failed to install Ctrl+C handler");
    }

    async fn wait_for_terminate() {
        #[cfg(not(unix))]
        std::future::pending::<()>().await;

        #[cfg(unix)]
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("ERROR: Failed to install SIGNAL handler")
            .recv()
            .await;
    }

    async fn wait_for_shutdown() {
        tokio::select! {
            () = Self::wait_for_ctrl_c() => {
                println!("\nINFO: Gracefully exiting (CTRL+C)");
            }
            () = Self::wait_for_terminate() => {
                println!("\nINFO: Gracefully exiting (SIGTERM)");
            }
        }
    }

    async fn bind_http(address: Option<&str>) -> Option<TcpListener> {
        let Some(address) = address else {
            println!("INFO: HTTP listener is not enabled... omitting");
            return None;
        };

        println!("INFO: Listening on http://{address}");
        Some(
            TcpListener::bind(address)
                .await
                .expect("failed to bind HTTP TCP listener"),
        )
    }

    fn bind_udp(address: Option<&str>) -> Option<UdpSocket> {
        let Some(address) = address else {
            println!("INFO: UDP listener is not enabled... omitting");
            return None;
        };

        println!("INFO: Listening on udp://{address}");

        let addr: std::net::SocketAddr =
            address.parse().expect("failed to parse UDP listen address");

        let socket2_sock = socket2::Socket::new(
            if addr.is_ipv6() {
                socket2::Domain::IPV6
            } else {
                socket2::Domain::IPV4
            },
            socket2::Type::DGRAM,
            Some(socket2::Protocol::UDP),
        )
        .expect("failed to create UDP socket2");

        let _ = socket2_sock.set_recv_buffer_size(4 * 1024 * 1024);
        let _ = socket2_sock.set_send_buffer_size(4 * 1024 * 1024);
        let _ = socket2_sock.set_nonblocking(true);

        socket2_sock
            .bind(&addr.into())
            .expect("failed to bind UDP socket with socket2");

        let std_socket: std::net::UdpSocket = socket2_sock.into();
        Some(UdpSocket::from_std(std_socket).expect("failed to convert to tokio UdpSocket"))
    }

    pub async fn run(self) {
        assert!(
            self.config.http_address.is_some() || self.config.udp_address.is_some(),
            "ERROR: At least one of HTTP or UDP listener must be enabled"
        );

        let database = DataProvider::from_config(
            self.config.database_path.as_deref(),
            self.config.database_backend,
        );
        let registry = Registry::new(database);
        let services = Arc::new(Services::new(&registry, &self.config));

        if let Some(listener) = Self::bind_http(self.config.http_address.as_deref()).await {
            WebDriver::new(listener)
                .with_cors_origins(self.config.cors_origins)
                .spawn(Arc::clone(&services));
        }

        if let Some(socket) = Self::bind_udp(self.config.udp_address.as_deref()) {
            WorkerDriver::new(socket)
                .with_worker_timeout(self.config.worker_timeout)
                .spawn(Arc::clone(&services));
        }

        Self::wait_for_shutdown().await;
    }
}

impl ApplicationBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: Config::from_env(),
        }
    }

    #[must_use]
    pub fn config(mut self, config: Config) -> Self {
        self.config = config;
        self
    }

    #[must_use]
    pub fn build(self) -> Application {
        Application {
            config: self.config,
        }
    }
}
