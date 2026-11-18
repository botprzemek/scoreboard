#![no_std]

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
