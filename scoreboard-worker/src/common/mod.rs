#[cfg(any(feature = "rp235x", feature = "esp32s3"))]
pub(crate) mod display;
#[cfg(any(feature = "rp235x", feature = "esp32s3"))]
pub(crate) mod net;
#[cfg(any(feature = "rp235x", feature = "esp32s3"))]
pub(crate) mod state;
#[cfg(any(feature = "rp235x", feature = "esp32s3"))]
pub(crate) mod traits;

#[cfg(any(feature = "rp235x", feature = "esp32s3"))]
pub use display::DisplayRunner;
#[cfg(any(feature = "rp235x", feature = "esp32s3"))]
pub use display::Hub75Display;
#[cfg(any(feature = "rp235x", feature = "esp32s3"))]
pub use net::dhcp;
#[cfg(any(feature = "rp235x", feature = "esp32s3"))]
pub use net::{
    ConnectedPacketResult, HandshakePacketResult, MASTER_UDP_PORT, WORKER_UDP_PORT, WorkerProtocol,
};
#[cfg(feature = "rp235x")]
pub use state::State;
#[cfg(any(feature = "rp235x", feature = "esp32s3"))]
pub use state::{Input, Machine};
#[cfg(any(feature = "rp235x", feature = "esp32s3"))]
pub use traits::BoardPeripherals;
#[cfg(feature = "linux")]
pub use traits::DisplayTarget;
#[cfg(any(feature = "rp235x", feature = "esp32s3"))]
pub use traits::{Clock, ConfigStore, StoredConfig};
