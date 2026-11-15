#![no_std]

#[cfg(feature = "std")]
extern crate std;

pub mod config;
#[cfg(any(feature = "flash", not(feature = "std")))]
pub mod flash;
pub mod print;

#[cfg(feature = "std")]
pub mod file;
#[cfg(feature = "std")]
pub mod keys;

#[cfg(feature = "std")]
pub use config::Input;
#[cfg(any(feature = "flash", not(feature = "std")))]
pub(crate) use config::Raw;
pub use config::{
    CONFIG_ADDR, CONFIG_SIZE, Config, ConfigBuilder, ConfigError, DeviceConfig, NetworkConfig,
    Verified,
};
#[cfg(feature = "serde")]
pub use file::JsonReader;
#[cfg(feature = "std")]
pub use file::{BinReader, BinWriter};
#[cfg(feature = "std")]
pub use keys::{KeyError, KeyPair, Keys, SecureKey};
pub use print::Print;

#[cfg(feature = "flash")]
pub use probe_rs;
