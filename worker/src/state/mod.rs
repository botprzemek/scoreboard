#[cfg(any(feature = "rp235x", feature = "linux"))]
mod handler;
mod machine;

#[cfg(any(feature = "rp235x", feature = "linux"))]
pub use handler::{
    BootHandler, CORE1_READY_SIGNAL, ErrorRecoveryHandler, NetworkingHandler, RunningHandler,
    StateHandler, SyncHandler,
};
#[cfg(any(feature = "rp235x", feature = "linux"))]
pub use machine::{Input, Machine, State};
pub use machine::{GAME_STATE, IS_CONNECTED_TO_MASTER};
