use crate::common::{Clock, ConfigStore, State, StoredConfig};
use crate::rp235x::peripherals::flash::write_config_to_flash;

use config::flash::FlashReader;
use config::{Config, ConfigBuilder, DeviceConfig, NetworkConfig};
use embassy_rp::Peri;
use embassy_rp::peripherals::FLASH;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;

pub static CORE1_READY_SIGNAL: Signal<CriticalSectionRawMutex, ()> = Signal::new();
pub static STATE_SIGNAL: Signal<CriticalSectionRawMutex, State> = Signal::new();

pub struct RpClock;

impl Default for RpClock {
    fn default() -> Self {
        Self::new()
    }
}

impl RpClock {
    pub const fn new() -> Self {
        Self
    }
}

impl Clock for RpClock {
    fn now_micros(&self) -> u64 {
        embassy_time::Instant::now().as_micros()
    }
}

pub struct FlashConfigStore<'d> {
    flash: Peri<'d, FLASH>,
}

impl<'d> FlashConfigStore<'d> {
    pub fn new(flash: Peri<'d, FLASH>) -> Self {
        Self { flash }
    }

    pub fn read_static() -> Option<StoredConfig> {
        let cfg = Config::read().ok()?;
        let sn = cfg.serial_number().ok()?;
        let id = cfg.device_id().ok()?;
        let ssid = cfg.wifi_ssid().ok()?;
        let pass = cfg.wifi_password().ok()?;
        StoredConfig::try_new(sn, id, ssid, pass)
    }
}

impl ConfigStore for FlashConfigStore<'_> {
    fn read(&mut self) -> Option<StoredConfig> {
        Self::read_static()
    }

    fn write(&mut self, cfg: &StoredConfig) -> Result<(), ()> {
        let mut builder = ConfigBuilder::new();
        builder
            .serial_number(cfg.serial_number.as_str())
            .device_id(cfg.device_id.as_str())
            .wifi_ssid(cfg.wifi_ssid.as_str())
            .wifi_password(cfg.wifi_password.as_str());

        let new_config = builder.build().map_err(|_| ())?;
        write_config_to_flash(self.flash.reborrow(), &new_config)
    }
}
