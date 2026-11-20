use core::error::Error;
use core::fmt::{Display, Formatter, Result};

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ConfigError {
    FileNotFound,
    InvalidEncoding,
    #[cfg(feature = "std")]
    InvalidKey,
    IntegrityCheckFailed,
    #[cfg(feature = "std")]
    Io(#[cfg_attr(feature = "serde", serde(skip))] std::io::ErrorKind),
    Serialization,
    StringTooLong,
}

impl Error for ConfigError {}

impl Display for ConfigError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> Result {
        write!(formatter, "{self:?}")
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
