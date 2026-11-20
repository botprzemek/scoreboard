use crate::common::state::IS_CONNECTED_TO_MASTER;
use crate::common::{DisplayRunner, Hub75BackBuffer, TRIPLE_DISPLAY};
use crate::rp235x::peripherals::LedPeripherals;
use crate::rp235x::platform::{CORE1_READY_SIGNAL, RpClock};

use core::sync::atomic::Ordering;
use embassy_time::Timer;
use scoreboard_hub75::LedOutputs;

pub struct Core1;

impl Core1 {
    pub fn entry(led: LedPeripherals) -> ! {
        let mut outputs = LedOutputs::from_peripherals(led);
        CORE1_READY_SIGNAL.signal(());
        defmt::info!("🚀 [Core 1] Display loop started (triple-buffered SIO)");

        loop {
            let buf = TRIPLE_DISPLAY.front_buffer();
            let brightness = if IS_CONNECTED_TO_MASTER.load(Ordering::Relaxed) {
                20
            } else {
                30
            };
            outputs.render_bcm_frame_fast(buf.bitplanes(), brightness);
        }
    }
}

/// Core 0 producer task: renders layouts and prepares bitplanes into the triple-buffer back slot.
#[embassy_executor::task]
pub async fn render_task() -> ! {
    let target = Hub75BackBuffer::new(&TRIPLE_DISPLAY, true);
    let mut runner = DisplayRunner::new(target, RpClock::new());

    loop {
        runner.step();
        Timer::after_millis(16).await;
    }
}
