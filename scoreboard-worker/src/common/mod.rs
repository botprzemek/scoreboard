pub(crate) mod display;
pub(crate) mod net;
pub(crate) mod state;
pub(crate) mod traits;

pub use display::DisplayRunner;
#[cfg(any(feature = "rp235x", feature = "esp32s3"))]
pub use display::{Hub75BackBuffer, TRIPLE_DISPLAY};
#[cfg(any(feature = "rp235x", feature = "esp32s3"))]
pub use net::dhcp;
#[allow(unused_imports)]
pub use net::{
    ConnectedPacketResult, HandshakePacketResult, MASTER_UDP_PORT, WORKER_UDP_PORT, WorkerProtocol,
};
#[allow(unused_imports)]
pub use state::{Input, Machine, State};
#[cfg(any(feature = "rp235x", feature = "esp32s3"))]
pub use traits::BoardPeripherals;
#[allow(unused_imports)]
pub use traits::{Clock, ConfigStore, DisplayTarget, StoredConfig};
