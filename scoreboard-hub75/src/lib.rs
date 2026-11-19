#![no_std]

#[cfg(any(
    all(feature = "w64h64", feature = "w64h32"),
    all(feature = "w64h64", feature = "w32h32"),
    all(feature = "w64h32", feature = "w32h32"),
))]
compile_error!("Enable at most one panel dimension feature: w64h64, w64h32, or w32h32.");

#[cfg(feature = "w64h64")]
pub const WIDTH: usize = 64;
#[cfg(feature = "w64h64")]
pub const HEIGHT: usize = 64;

#[cfg(feature = "w32h32")]
pub const WIDTH: usize = 32;
#[cfg(feature = "w32h32")]
pub const HEIGHT: usize = 32;

#[cfg(all(not(feature = "w64h64"), not(feature = "w32h32")))]
pub const WIDTH: usize = 64;
#[cfg(all(not(feature = "w64h64"), not(feature = "w32h32")))]
pub const HEIGHT: usize = 32;

pub const HALF_HEIGHT: usize = HEIGHT / 2;

mod bitplane;
mod color;
mod display;
mod font;
mod led;
mod output;

#[cfg(feature = "esp32s3")]
pub mod esp32s3;
#[cfg(feature = "rp235x")]
pub mod rp235x;

pub use color::{BLACK, BLUE, CORAL_RED, Color, GREEN, RED, WHITE, hue_to_rgb, with_brightness};
pub use display::DisplayBuffer;
pub use output::{LedOutputs, OutputPin, PinLevel};

#[cfg(feature = "esp32s3")]
pub use esp32s3::LedPeripherals;
#[cfg(feature = "rp235x")]
pub use rp235x::LedPeripherals;
