use crate::common::{DisplayRunner, Hub75BackBuffer, TRIPLE_DISPLAY};
use crate::esp32s3::platform::EspClock;

use embassy_time::Timer;
use scoreboard_hub75::LedPeripherals;
use scoreboard_hub75::esp32s3::Hub75Dma;

pub struct Core1;

impl Core1 {
    pub fn entry(led: LedPeripherals) -> ! {
        let mut outputs = Hub75Dma::new(led);

        loop {
            let buf = TRIPLE_DISPLAY.front_buffer();
            outputs.render_bcm_frame_fast(buf.bitplanes(), 20);
        }
    }
}

/// Core 0 producer task: renders layouts and prepares bitplanes into the triple-buffer back slot.
#[embassy_executor::task]
pub async fn render_task() -> ! {
    let target = Hub75BackBuffer::new(&TRIPLE_DISPLAY, false);
    let mut runner = DisplayRunner::new(target, EspClock::new()).with_persisted_tick(true);

    loop {
        runner.step();
        Timer::after_millis(16).await;
    }
}
