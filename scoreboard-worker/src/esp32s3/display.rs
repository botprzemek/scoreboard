use crate::esp32s3::platform::EspClock;
use crate::common::{DisplayRunner, Hub75Display};

use scoreboard_hub75::{DisplayBuffer, LedOutputs, LedPeripherals, HALF, HALF_HEIGHT, WIDTH};
use static_cell::StaticCell;

static DISPLAY: StaticCell<DisplayBuffer<WIDTH, HALF, HALF_HEIGHT>> = StaticCell::new();

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
