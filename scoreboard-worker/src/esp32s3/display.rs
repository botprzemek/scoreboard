use crate::common::{DisplayRunner, Hub75Display};
use crate::esp32s3::platform::EspClock;

use hub75::{DisplayBuffer, LedOutputs, LedPeripherals};
use static_cell::StaticCell;

static DISPLAY: StaticCell<DisplayBuffer<64, 32, 16>> = StaticCell::new();

pub struct Core1;

impl Core1 {
    pub fn entry(led: LedPeripherals) -> ! {
        let mut outputs = LedOutputs::from_peripherals(led);
        let display_buf = DISPLAY.init_with(DisplayBuffer::new);

        let target = Hub75Display::new(display_buf, false, |buf, _is_connected| {
            outputs.render_bcm_frame_esp32s3_fast(buf.bitplanes(), 25);
        });

        let mut runner = DisplayRunner::new(target, EspClock::new());
        loop {
            runner.step();
        }
    }
}
