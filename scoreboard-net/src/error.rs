use core::error::Error;
use core::fmt::{Display, Formatter, Result};

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum NetError {
    Disconnected,
    InvalidPacket(&'static str),
}

impl Display for NetError {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result {
        write!(fmt, "{self:?}")
    }
}

impl Error for NetError {}
