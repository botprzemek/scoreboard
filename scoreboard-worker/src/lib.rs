#![cfg_attr(not(feature = "linux"), no_std)]

mod common;

#[cfg(feature = "linux")]
pub mod linux;

#[cfg(feature = "esp32s3")]
pub mod esp32s3;

#[cfg(feature = "rp235x")]
pub mod rp235x;
