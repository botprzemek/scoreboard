#![cfg_attr(any(feature = "rp235x", feature = "esp32s3"), no_std)]

#[cfg(any(feature = "rp235x", feature = "esp32s3", feature = "linux"))]
pub mod core0;
#[cfg(any(feature = "rp235x", feature = "esp32s3", feature = "linux"))]
pub mod core1;
#[cfg(any(feature = "rp235x", feature = "esp32s3", feature = "linux"))]
pub mod state;

#[cfg(feature = "rp235x")]
pub mod interrupts;
#[cfg(feature = "rp235x")]
pub mod peripherals;
