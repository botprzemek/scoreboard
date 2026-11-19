pub mod dhcp;
pub mod display;
pub mod layout;
pub mod protocol;
pub mod state;
pub mod traits;

pub use display::{DisplayRunner, Hub75Display, idle_palette_color, tick_game_in_progress};
pub use layout::ScoreboardLayout;
pub use protocol::{
    ConnectedPacketResult, HandshakePacketResult, MASTER_UDP_PORT, WORKER_UDP_PORT, WorkerProtocol,
};
pub use state::{GAME_STATE, IS_CONNECTED_TO_MASTER, Input, Machine, SharedGameState, State};
pub use traits::{BoardPeripherals, Clock, ConfigStore, DisplayTarget, StoredConfig};
