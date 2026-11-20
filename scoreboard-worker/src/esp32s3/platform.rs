use crate::common::{Clock, ConfigStore, StoredConfig};

use embedded_storage::{ReadStorage, Storage};
use esp_hal::peripherals::FLASH;
use esp_storage::FlashStorage;
use scoreboard_config::{CONFIG_SIZE, Config, ConfigBuilder, DeviceConfig, NetworkConfig};

const CONFIG_FLASH_OFFSET: u32 = 0x3F_0000;
const CONFIG_RECORD_LEN: usize = 256;

pub struct EspClock {
    start: esp_hal::time::Instant,
}

impl Default for EspClock {
    fn default() -> Self {
        Self::new()
    }
}

impl EspClock {
    pub fn new() -> Self {
        Self {
            start: esp_hal::time::Instant::now(),
        }
    }
}

impl Clock for EspClock {
    fn now_micros(&self) -> u64 {
        (esp_hal::time::Instant::now() - self.start).as_micros()
    }
}

pub struct FlashConfigStore<'d> {
    flash: FlashStorage<'d>,
}

impl<'d> FlashConfigStore<'d> {
    pub fn new(flash_peri: FLASH<'d>) -> Self {
        Self {
            flash: FlashStorage::new(flash_peri),
        }
    }
}

impl ConfigStore for FlashConfigStore<'_> {
    fn read(&mut self) -> Option<StoredConfig> {
        #[repr(C, align(4))]
        struct AlignedBuf([u8; CONFIG_RECORD_LEN]);

        let mut buf = AlignedBuf([0xFFu8; CONFIG_RECORD_LEN]);
        if let Err(e) = self.flash.read(CONFIG_FLASH_OFFSET, &mut buf.0) {
            log::error!("Flash read failed at {CONFIG_FLASH_OFFSET:#x}: {:?}", e);
            return None;
        }

        let cfg = Config::from_verified_bytes(&buf.0[..CONFIG_SIZE]).ok()?;
        let sn = cfg.serial_number().ok()?;
        let id = cfg.device_id().ok()?;
        let ssid = cfg.wifi_ssid().ok()?;
        let pass = cfg.wifi_password().ok()?;

        StoredConfig::try_new(sn, id, ssid, pass)
    }

    fn write(&mut self, cfg: &StoredConfig) -> Result<(), ()> {
        let mut builder = ConfigBuilder::new();
        builder
            .serial_number(cfg.serial_number.as_str())
            .device_id(cfg.device_id.as_str())
            .wifi_ssid(cfg.wifi_ssid.as_str())
            .wifi_password(cfg.wifi_password.as_str());

        let new_config = builder.build().map_err(|_| ())?;
        let bytes = new_config.as_bytes();
        if bytes.len() > CONFIG_RECORD_LEN {
            return Err(());
        }

        let mut buf = [0xFFu8; CONFIG_RECORD_LEN];
        buf[..bytes.len()].copy_from_slice(bytes);

        if let Err(e) = self.flash.write(CONFIG_FLASH_OFFSET, &buf) {
            log::error!("Flash write failed at {CONFIG_FLASH_OFFSET:#x}: {:?}", e);
            return Err(());
        }

        let mut verify = [0u8; CONFIG_RECORD_LEN];
        if let Err(e) = self.flash.read(CONFIG_FLASH_OFFSET, &mut verify) {
            log::error!("Flash verify read failed: {:?}", e);
            return Err(());
        }
        if verify != buf {
            log::error!("Flash verify mismatch after write!");
            return Err(());
        }

        Ok(())
    }
}
