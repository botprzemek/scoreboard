#[cfg(any(feature = "rp235x", feature = "esp32s3"))]
pub mod dhcp;
mod protocol;

pub use protocol::{ConnectedPacketResult, HandshakePacketResult, WorkerProtocol};
pub use scoreboard_net::{MASTER_UDP_PORT, WORKER_UDP_PORT};
