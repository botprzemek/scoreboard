use core::fmt;
use core::marker::PhantomData;
use core::mem;
use core::slice;
use core::str;
use core::sync::atomic::{self, Ordering};

pub const CONFIG_SIZE: usize = mem::size_of::<Config>();

pub const CONFIG_ADDR: usize = 0x1003_f000;

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigError {
    FileNotFound,
    InvalidEncoding,
    #[cfg(feature = "std")]
    InvalidKey,
    IntegrityCheckFailed,
    #[cfg(feature = "std")]
    Io(std::io::ErrorKind),
    Serialization,
    StringTooLong,
}

#[cfg(all(feature = "std", feature = "serde"))]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Input {
    pub(crate) serial_number: std::string::String,
    pub(crate) device_id: std::string::String,
    pub(crate) wifi_ssid: std::string::String,
    pub(crate) wifi_password: std::string::String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConfigField<const N: usize>([u8; N]);

#[derive(Debug, Clone, PartialEq, Eq)]
struct SerialNumber(ConfigField<32>);

#[derive(Debug, Clone, PartialEq, Eq)]
struct DeviceIdentifier(ConfigField<32>);

#[derive(Debug, Clone, PartialEq, Eq)]
struct WifiSsid(ConfigField<32>);

#[derive(Debug, Clone, PartialEq, Eq)]
struct WifiPassword(ConfigField<64>);

#[derive(Debug, Clone, PartialEq, Eq)]
struct Checksum(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Raw;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Verified;

#[repr(C, align(4))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config<State = Verified> {
    serial_number: SerialNumber,
    device_id: DeviceIdentifier,
    wifi_ssid: WifiSsid,
    wifi_password: WifiPassword,
    checksum: Checksum,
    _state: PhantomData<State>,
}

pub trait DeviceConfig {
    fn serial_number(&self) -> Result<&str, ConfigError>;

    fn device_id(&self) -> Result<&str, ConfigError>;
}

pub trait NetworkConfig {
    fn wifi_ssid(&self) -> Result<&str, ConfigError>;

    fn wifi_password(&self) -> Result<&str, ConfigError>;
}

#[cfg(feature = "std")]
impl std::error::Error for ConfigError {}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FileNotFound => write!(formatter, "File not found"),
            Self::InvalidEncoding => {
                write!(formatter, "String slice contains invalid UTF-8 sequence")
            }
            #[cfg(feature = "std")]
            Self::InvalidKey => write!(formatter, "Cryptographic key configuration is invalid"),
            Self::IntegrityCheckFailed => write!(formatter, "Memory checksum validation failed"),
            #[cfg(feature = "std")]
            Self::Io(error) => write!(formatter, "I/O Error: {error}"),
            Self::Serialization => write!(formatter, "Serialization/Deserialization failed"),
            Self::StringTooLong => {
                write!(formatter, "Input string exceeds maximum buffer capacity")
            }
        }
    }
}

#[cfg(feature = "std")]
impl From<crate::keys::KeyError> for ConfigError {
    fn from(_: crate::keys::KeyError) -> Self {
        Self::InvalidKey
    }
}

#[cfg(feature = "std")]
impl From<std::io::Error> for ConfigError {
    fn from(error: std::io::Error) -> Self {
        match error.kind() {
            std::io::ErrorKind::NotFound => Self::FileNotFound,
            other => Self::Io(other),
        }
    }
}

impl Default for Config<Verified> {
    fn default() -> Self {
        let mut config = Self {
            serial_number: SerialNumber(ConfigField::new()),
            device_id: DeviceIdentifier(ConfigField::new()),
            wifi_ssid: WifiSsid(ConfigField::new()),
            wifi_password: WifiPassword(ConfigField::new()),
            checksum: Checksum(0),
            _state: PhantomData,
        };

        let _ = config.serial_number.0.copy_from_slice(b"0000-0000");
        let _ = config.device_id.0.copy_from_slice(b"rp235x");
        let _ = config.wifi_ssid.0.copy_from_slice(b"my_ssid");
        let _ = config.wifi_password.0.copy_from_slice(b"secret_password");

        config.update_checksum();
        config
    }
}

impl<State> Drop for Config<State> {
    fn drop(&mut self) {
        self.zeroize();
        atomic::compiler_fence(Ordering::SeqCst);
    }
}

impl DeviceConfig for Config<Verified> {
    fn serial_number(&self) -> Result<&str, ConfigError> {
        self.serial_number.0.parse_str()
    }

    fn device_id(&self) -> Result<&str, ConfigError> {
        self.device_id.0.parse_str()
    }
}

impl<T: DeviceConfig> DeviceConfig for &T {
    fn serial_number(&self) -> Result<&str, ConfigError> {
        (**self).serial_number()
    }

    fn device_id(&self) -> Result<&str, ConfigError> {
        (**self).device_id()
    }
}

impl NetworkConfig for Config {
    fn wifi_ssid(&self) -> Result<&str, ConfigError> {
        self.wifi_ssid.0.parse_str()
    }

    fn wifi_password(&self) -> Result<&str, ConfigError> {
        self.wifi_password.0.parse_str()
    }
}

impl<const N: usize> Default for ConfigField<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> ConfigField<N> {
    #[must_use]
    const fn new() -> Self {
        Self([0; N])
    }

    #[inline]
    fn copy_from_slice(&mut self, source: &[u8]) -> Result<(), ConfigError> {
        if source.len() >= N {
            return Err(ConfigError::StringTooLong);
        }

        self.0.fill(0);
        self.0[..source.len()].copy_from_slice(source);

        Ok(())
    }

    #[inline]
    fn parse_str(&self) -> Result<&str, ConfigError> {
        let length = self
            .0
            .iter()
            .position(|&byte| byte == 0)
            .ok_or(ConfigError::Serialization)?;

        str::from_utf8(&self.0[..length]).map_err(|_| ConfigError::InvalidEncoding)
    }

    #[inline]
    fn zeroize(&mut self) {
        self.0.fill(0);
    }
}

impl<State> Config<State> {
    pub(crate) fn try_new(
        serial_number: &str,
        device_id: &str,
        wifi_ssid: &str,
        wifi_password: &str,
    ) -> Result<Self, ConfigError> {
        let mut config = Self {
            serial_number: SerialNumber(ConfigField::new()),
            device_id: DeviceIdentifier(ConfigField::new()),
            wifi_ssid: WifiSsid(ConfigField::new()),
            wifi_password: WifiPassword(ConfigField::new()),
            checksum: Checksum(0),
            _state: PhantomData,
        };

        if let Err(error) = config
            .serial_number
            .0
            .copy_from_slice(serial_number.as_bytes())
        {
            config.zeroize();
            return Err(error);
        }
        if let Err(error) = config.device_id.0.copy_from_slice(device_id.as_bytes()) {
            config.zeroize();
            return Err(error);
        }
        if let Err(error) = config.wifi_ssid.0.copy_from_slice(wifi_ssid.as_bytes()) {
            config.zeroize();
            return Err(error);
        }
        if let Err(error) = config
            .wifi_password
            .0
            .copy_from_slice(wifi_password.as_bytes())
        {
            config.zeroize();
            return Err(error);
        }

        config.update_checksum();
        Ok(config)
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        unsafe { slice::from_raw_parts((self as *const Self).cast::<u8>(), CONFIG_SIZE) }
    }

    #[inline(always)]
    fn calculate_checksum(data: &[u8]) -> Checksum {
        let mut accumulator: u64 = 0x517c_c1b7_2722_0a95;

        for &byte in data {
            accumulator = accumulator.wrapping_mul(31).wrapping_add(u64::from(byte));
        }

        Checksum((accumulator ^ (accumulator >> 32)) as u32)
    }

    fn update_checksum(&mut self) {
        let slice_len = CONFIG_SIZE - mem::size_of::<u32>();
        let pointer = (self as *const Self).cast::<u8>();

        let data = unsafe { slice::from_raw_parts(pointer, slice_len) };

        self.checksum = Self::calculate_checksum(data);
    }

    #[must_use]
    pub(crate) fn verify_integrity(&self) -> bool {
        let slice_len = CONFIG_SIZE - mem::size_of::<u32>();
        let pointer = (self as *const Self).cast::<u8>();

        let data = unsafe { slice::from_raw_parts(pointer, slice_len) };

        let computed = Self::calculate_checksum(data);

        let mut difference = computed.0 ^ self.checksum.0;
        difference |= difference >> 16;
        difference |= difference >> 8;
        difference |= difference >> 4;
        difference |= difference >> 2;
        difference |= difference >> 1;

        (difference & 1) == 0
    }

    fn zeroize(&mut self) {
        self.serial_number.0.zeroize();
        self.device_id.0.zeroize();
        self.wifi_ssid.0.zeroize();
        self.wifi_password.0.zeroize();
        self.checksum = Checksum(0);

        atomic::compiler_fence(Ordering::SeqCst);
    }
}

impl Config<Raw> {
    pub(crate) fn from_bytes(bytes: &[u8]) -> Result<&Self, ConfigError> {
        if bytes.len() < CONFIG_SIZE {
            return Err(ConfigError::Serialization);
        }

        let pointer = bytes.as_ptr();
        if !(pointer as usize).is_multiple_of(mem::align_of::<Self>()) {
            return Err(ConfigError::Serialization);
        }

        let config_ref = unsafe { &*pointer.cast::<Self>() };

        Ok(config_ref)
    }

    pub(crate) fn verify(&self) -> Result<&Config<Verified>, ConfigError> {
        if !self.verify_integrity() {
            return Err(ConfigError::IntegrityCheckFailed);
        }

        let verified_ref = unsafe { &*(self as *const Self).cast::<Config<Verified>>() };

        Ok(verified_ref)
    }
}
