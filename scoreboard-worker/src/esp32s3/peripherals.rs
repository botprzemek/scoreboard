use crate::common::BoardPeripherals;

use esp_hal::peripherals::{CPU_CTRL, FLASH, Peripherals, SW_INTERRUPT, TIMG0, WIFI};
use scoreboard_hub75::LedPeripherals;

pub struct EspBoardPeripherals {
    pub led: LedPeripherals,
    pub wifi: WIFI<'static>,
    pub flash: FLASH<'static>,
    pub cpu_ctrl: CPU_CTRL<'static>,
    pub sw_interrupt: SW_INTERRUPT<'static>,
    pub timg0: TIMG0<'static>,
}

impl BoardPeripherals for EspBoardPeripherals {
    fn init() -> Self {
        split(esp_hal::init(esp_hal::Config::default()))
    }
}

#[must_use]
pub fn init() -> EspBoardPeripherals {
    EspBoardPeripherals::init()
}

#[must_use]
pub fn split(peripherals: Peripherals) -> EspBoardPeripherals {
    let led = LedPeripherals::new(
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

    EspBoardPeripherals {
        led,
        wifi: peripherals.WIFI,
        flash: peripherals.FLASH,
        cpu_ctrl: peripherals.CPU_CTRL,
        sw_interrupt: peripherals.SW_INTERRUPT,
        timg0: peripherals.TIMG0,
    }
}
