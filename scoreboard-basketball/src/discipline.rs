use crate::{BasketballError, Quarter};

#[non_exhaustive]
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Discipline {
    Fiba5v5 = 0x00,
    Fiba3v3 = 0x01,
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisciplineConfig {
    pub initial_quarter: Quarter,
    pub regulation_millis: u32,
    pub clock_millis: u32,
}

impl TryFrom<u8> for Discipline {
    type Error = BasketballError;

    fn try_from(discipline: u8) -> Result<Self, Self::Error> {
        match discipline {
            0x00 => Ok(Self::Fiba5v5),
            0x01 => Ok(Self::Fiba3v3),
            other => Err(BasketballError::InvalidDiscipline(other)),
        }
    }
}

impl Discipline {
    #[must_use]
    pub fn config(&self) -> DisciplineConfig {
        match self {
            Self::Fiba5v5 => DisciplineConfig {
                initial_quarter: Quarter::Q1,
                regulation_millis: 600_000,
                clock_millis: 24_000,
            },
            Self::Fiba3v3 => DisciplineConfig {
                initial_quarter: Quarter::None,
                regulation_millis: 600_000,
                clock_millis: 12_000,
            },
        }
    }
}
