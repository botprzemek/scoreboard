use crate::common::{Clock, ConfigStore, StoredConfig};

use embedded_storage::{ReadStorage, Storage};
use esp_hal::peripherals::FLASH;
use esp_storage::FlashStorage;

const CONFIG_FLASH_OFFSET: u32 = 0x3F_0000;
const CONFIG_MAGIC: [u8; 4] = *b"SBC1";
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
        let mut buf = [0xFFu8; CONFIG_RECORD_LEN];
        if let Err(e) = self.flash.read(CONFIG_FLASH_OFFSET, &mut buf) {
            log::error!("Flash read failed at {CONFIG_FLASH_OFFSET:#x}: {:?}", e);
            return None;
        }
        if buf[..4] != CONFIG_MAGIC {
            log::warn!("No valid config in flash (magic={:?})", &buf[..4]);
            return None;
        }

        let sn_len = usize::from(buf[4]);
        let id_len = usize::from(buf[5]);
        let ssid_len = usize::from(buf[6]);
        let pass_len = usize::from(buf[7]);

        let total = 8usize + sn_len + id_len + ssid_len + pass_len;
        if total > CONFIG_RECORD_LEN {
            return None;
        }

        let mut pos = 8usize;
        let sn_str = core::str::from_utf8(&buf[pos..pos + sn_len]).ok()?;
        pos += sn_len;
        let id_str = core::str::from_utf8(&buf[pos..pos + id_len]).ok()?;
        pos += id_len;
        let ssid_str = core::str::from_utf8(&buf[pos..pos + ssid_len]).ok()?;
        pos += ssid_len;
        let pass_str = core::str::from_utf8(&buf[pos..pos + pass_len]).ok()?;

        StoredConfig::try_new(sn_str, id_str, ssid_str, pass_str)
    }

    fn write(&mut self, cfg: &StoredConfig) -> Result<(), ()> {
        let sn_b = cfg.serial_number.as_bytes();
        let id_b = cfg.device_id.as_bytes();
        let ssid_b = cfg.wifi_ssid.as_bytes();
        let pass_b = cfg.wifi_password.as_bytes();

        if sn_b.len() > u8::MAX as usize
            || id_b.len() > u8::MAX as usize
            || ssid_b.len() > u8::MAX as usize
            || pass_b.len() > u8::MAX as usize
        {
            return Err(());
        }

        let total = 8 + sn_b.len() + id_b.len() + ssid_b.len() + pass_b.len();
        if total > CONFIG_RECORD_LEN {
            return Err(());
        }

        let mut buf = [0xFFu8; CONFIG_RECORD_LEN];
        buf[..4].copy_from_slice(&CONFIG_MAGIC);
        buf[4] = sn_b.len() as u8;
        buf[5] = id_b.len() as u8;
        buf[6] = ssid_b.len() as u8;
        buf[7] = pass_b.len() as u8;

        let mut pos = 8usize;
        buf[pos..pos + sn_b.len()].copy_from_slice(sn_b);
        pos += sn_b.len();
        buf[pos..pos + id_b.len()].copy_from_slice(id_b);
        pos += id_b.len();
        buf[pos..pos + ssid_b.len()].copy_from_slice(ssid_b);
        pos += ssid_b.len();
        buf[pos..pos + pass_b.len()].copy_from_slice(pass_b);

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
