#![no_std]
#![no_main]

use core::panic::PanicInfo;
use core::ptr::addr_of_mut;

use embassy_executor::Spawner;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::system::{CpuControl, Stack};
use esp_hal::timer::timg::TimerGroup;
use hub75::LedPeripherals;
use scoreboard_worker::core0::esp32s3::run_esp32s3_core0;
use scoreboard_worker::core1::Core1;

esp_bootloader_esp_idf::esp_app_desc!();

static mut APP_CORE_STACK: Stack<16384> = Stack::new();

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    esp_println::logger::init_logger(log::LevelFilter::Info);
    log::info!("=== ESP32-S3 Worker starting ===");

    let peripherals = esp_hal::init(esp_hal::Config::default());

    esp_alloc::heap_allocator!(size: 72 * 1024);

    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    let led_peripherals = LedPeripherals::new(
        peripherals.GPIO42,
        peripherals.GPIO41,
        peripherals.GPIO40,
        peripherals.GPIO38,
        peripherals.GPIO39,
        peripherals.GPIO37,
        peripherals.GPIO45,
        peripherals.GPIO36,
        peripherals.GPIO48,
        peripherals.GPIO35,
        peripherals.GPIO21,
        peripherals.GPIO2,
        peripherals.GPIO47,
        peripherals.GPIO14,
    );

    let mut cpu_control = CpuControl::new(peripherals.CPU_CTRL);
    let app_core_guard = unsafe {
        cpu_control
            .start_app_core(&mut *addr_of_mut!(APP_CORE_STACK), move || {
                Core1::entry(led_peripherals)
            })
            .expect("failed to start Core 1")
    };

    run_esp32s3_core0(spawner, peripherals.WIFI, peripherals.FLASH, app_core_guard).await
}
