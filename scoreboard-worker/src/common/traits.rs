use heapless::String;
use scoreboard_basketball::Game;
use scoreboard_config::ConfigBuilder;
use scoreboard_hub75::Color;
use scoreboard_net::{Device, DeviceConfigPacket, DeviceType};

/// Platform peripheral initialization and splitting abstraction.
pub trait BoardPeripherals: Sized {
    fn init() -> Self;
}

/// Monotonic platform clock in microseconds.
pub trait Clock {
    fn now_micros(&self) -> u64;
}

/// Platform display target driven by [`crate::common::display::DisplayRunner`].
pub trait DisplayTarget {
    /// Whether the target displays an animated idle screen while disconnected from Master.
    fn supports_idle_animation(&self) -> bool {
        false
    }

    /// Update the display with the latest game state and master connection status.
    /// Returns `true` if the frame was actually rendered (or `false` if throttled).
    fn update(&mut self, game: &Game, is_connected: bool) -> bool;

    /// Update the display with an idle solid color frame while disconnected.
    fn update_idle(&mut self, _color: Color) {}

    /// Refresh the physical output (e.g. HUB75 BCM scan or terminal throttle).
    fn refresh(&mut self, _is_connected: bool) {}
}

/// Platform-agnostic worker configuration payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredConfig {
    pub serial_number: String<32>,
    pub device_id: String<32>,
    pub wifi_ssid: String<64>,
    pub wifi_password: String<64>,
}

impl StoredConfig {
    /// Parse and validate a [`DeviceConfigPacket`] into a [`StoredConfig`].
    pub fn from_packet(packet: &DeviceConfigPacket) -> Option<Self> {
        let sn = packet.serial_number().ok()?;
        let id = packet.device_id().ok()?;
        let ssid = packet.wifi_ssid().ok()?;
        let pass = packet.wifi_password().ok()?;

        Self::try_new(sn, id, ssid, pass)
    }

    /// Validate fields via [`ConfigBuilder`] and construct a [`StoredConfig`].
    pub fn try_new(sn: &str, id: &str, ssid: &str, pass: &str) -> Option<Self> {
        let mut builder = ConfigBuilder::new();
        builder
            .serial_number(sn)
            .device_id(id)
            .wifi_ssid(ssid)
            .wifi_password(pass);
        builder.build().ok()?;

        let mut serial_number = String::new();
        let mut device_id = String::new();
        let mut wifi_ssid = String::new();
        let mut wifi_password = String::new();

        serial_number.push_str(sn).ok()?;
        device_id.push_str(id).ok()?;
        wifi_ssid.push_str(ssid).ok()?;
        wifi_password.push_str(pass).ok()?;

        Some(Self {
            serial_number,
            device_id,
            wifi_ssid,
            wifi_password,
        })
    }

    /// Build a network [`Device`] descriptor for UDP handshakes.
    pub fn to_device(&self) -> Option<Device> {
        Device::try_new(
            DeviceType::Worker,
            self.serial_number.as_str(),
            self.device_id.as_str(),
        )
        .ok()
    }

    /// Build the AP setup hotspot SSID from the serial number, falling back to `"Worker-Setup"`.
    pub fn ap_ssid(config: Option<&Self>) -> String<32> {
        let mut ssid = String::<32>::new();
        if let Some(cfg) = config
            && !cfg.serial_number.is_empty()
        {
            let _ = ssid.push_str(cfg.serial_number.as_str());
        }
        if ssid.is_empty() {
            let _ = ssid.push_str("Worker-Setup");
        }
        ssid
    }
}

/// Persistent (or in-memory) configuration storage abstraction.
pub trait ConfigStore {
    fn read(&mut self) -> Option<StoredConfig>;

    #[allow(clippy::result_unit_err)]
    fn write(&mut self, config: &StoredConfig) -> Result<(), ()>;
}
