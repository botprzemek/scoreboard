#![no_std]
#![no_main]

use core::panic::PanicInfo;
use core::ptr::addr_of_mut;

use scoreboard_worker::BoardPeripherals as _;
use scoreboard_worker::esp32s3::peripherals::EspBoardPeripherals;
use scoreboard_worker::esp32s3::{Core0, Core1};

use embassy_executor::Spawner;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::system::{CpuControl, Stack};
use esp_hal::timer::timg::TimerGroup;

esp_bootloader_esp_idf::esp_app_desc!();

static mut APP_CORE_STACK: Stack<16384> = Stack::new();

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    let board_peripherals = EspBoardPeripherals::init();

    esp_println::logger::init_logger(log::LevelFilter::Info);

    esp_alloc::heap_allocator!(size: 72 * 1024);

    let sw_int = SoftwareInterruptControl::new(board_peripherals.sw_interrupt);
    let timg0 = TimerGroup::new(board_peripherals.timg0);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    let led_peripherals = board_peripherals.led;
    let mut cpu_control = CpuControl::new(board_peripherals.cpu_ctrl);
    let app_core_guard = unsafe {
        cpu_control
            .start_app_core(&mut *addr_of_mut!(APP_CORE_STACK), move || {
                Core1::entry(led_peripherals)
            })
            .expect("failed to start Core 1")
    };

    Core0::run(
        spawner,
        board_peripherals.wifi,
        board_peripherals.flash,
        app_core_guard,
    )
    .await
}
