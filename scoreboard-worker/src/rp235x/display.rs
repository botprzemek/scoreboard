use crate::common::{DisplayRunner, Hub75Display};
use crate::rp235x::platform::{CORE1_READY_SIGNAL, RpClock};

use embassy_executor::{Executor, Spawner};
use embassy_futures::yield_now;
use scoreboard_hub75::{DisplayBuffer, LedOutputs, LedPeripherals};
use static_cell::StaticCell;

pub static EXECUTOR: StaticCell<Executor> = StaticCell::new();
static DISPLAY: StaticCell<DisplayBuffer> = StaticCell::new();

pub struct Core1;

impl Core1 {
    pub fn entry(led: LedPeripherals) -> ! {
        let executor = Executor::new();

        EXECUTOR.init(executor).run(|spawner| {
            spawner.spawn(defmt::unwrap!(display_task(spawner, led)));
            CORE1_READY_SIGNAL.signal(());
        })
    }
}

#[embassy_executor::task]
async fn display_task(_spawner: Spawner, peripherals: LedPeripherals) {
    let mut outputs = LedOutputs::from_peripherals(peripherals);
    let display_buf = DISPLAY.init_with(DisplayBuffer::new);

    let target = Hub75Display::new(display_buf, true, |buf, is_connected| {
        let brightness = if is_connected { 20 } else { 30 };
        outputs.render_bcm_frame_sio_fast(buf.bitplanes(), brightness);
    });

    let mut runner = DisplayRunner::new(target, RpClock::new());
    defmt::info!("🚀 [Core 1] Display runner started (Hardware SIO / DMA Engine)");

    loop {
        runner.step();
        yield_now().await;
    }
}
