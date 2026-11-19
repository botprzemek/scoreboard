use core::error::Error;
use core::fmt::{Display, Formatter, Result};

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum BasketballError {
    InvalidState(u8),
    InvalidDiscipline(u8),
    InvalidQuarter(u8),
    InvalidScore(u8),
    InvalidTeam(u8),
    InvalidPayloadLength,
}

impl Display for BasketballError {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result {
        write!(fmt, "{self:?}")
    }
}

impl Error for BasketballError {}
