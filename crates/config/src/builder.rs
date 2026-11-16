use crate::{Config, ConfigError, Verified};

#[derive(Default)]
pub struct ConfigBuilder<'input> {
    serial_number: Option<&'input str>,
    device_id: Option<&'input str>,
    wifi_ssid: Option<&'input str>,
    wifi_password: Option<&'input str>,
}

impl<'input> ConfigBuilder<'input> {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn serial_number(&mut self, serial_number: &'input str) -> &mut Self {
        self.serial_number = Some(serial_number);
        self
    }

    pub fn device_id(&mut self, device_id: &'input str) -> &mut Self {
        self.device_id = Some(device_id);
        self
    }

    pub fn wifi_ssid(&mut self, wifi_ssid: &'input str) -> &mut Self {
        self.wifi_ssid = Some(wifi_ssid);
        self
    }

    pub fn wifi_password(&mut self, wifi_password: &'input str) -> &mut Self {
        self.wifi_password = Some(wifi_password);
        self
    }

    pub fn reset(&mut self) -> &mut Self {
        self.serial_number = None;
        self.device_id = None;
        self.wifi_ssid = None;
        self.wifi_password = None;
        self
    }

    pub fn build(&self) -> Result<Config<Verified>, ConfigError> {
        Config::<Verified>::try_new(
            self.serial_number.ok_or(ConfigError::Serialization)?,
            self.device_id.ok_or(ConfigError::Serialization)?,
            self.wifi_ssid.ok_or(ConfigError::Serialization)?,
            self.wifi_password.ok_or(ConfigError::Serialization)?,
        )
    }
}
