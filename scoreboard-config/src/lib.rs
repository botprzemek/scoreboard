#![no_std]

#[cfg(feature = "std")]
extern crate std;

mod builder;
mod config;

pub mod print;

#[cfg(any(feature = "flash", not(feature = "std")))]
pub mod flash;

#[cfg(feature = "std")]
pub mod file;
#[cfg(feature = "std")]
mod keys;

pub use builder::ConfigBuilder;
#[cfg(all(feature = "std", feature = "serde"))]
pub(crate) use config::Input;
#[cfg(any(feature = "flash", not(feature = "std")))]
pub(crate) use config::Raw;
pub use config::{
    CONFIG_ADDR, CONFIG_SIZE, Config, ConfigError, DeviceConfig, NetworkConfig, Verified,
};
#[cfg(feature = "serde")]
pub use file::JsonReader;
#[cfg(feature = "std")]
pub use file::{BinReader, BinWriter};
#[cfg(any(feature = "flash", not(feature = "std")))]
pub use flash::{FlashReader, FlashWriter};
#[cfg(feature = "std")]
pub(crate) use keys::Keys;
pub use print::Print;

#[cfg(feature = "flash")]
pub use probe_rs;
