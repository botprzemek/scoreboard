#[non_exhaustive]
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum State {
    Scheduled = 0x00,
    QuarterStart = 0x01,
    InProgress = 0x02,
    Paused = 0x03,
    QuarterEnd = 0x04,
    Final = 0x05,
}

impl TryFrom<u8> for State {
    type Error = &'static str;

    fn try_from(state: u8) -> Result<Self, Self::Error> {
        match state {
            0x00 => Ok(Self::Scheduled),
            0x01 => Ok(Self::QuarterStart),
            0x02 => Ok(Self::InProgress),
            0x03 => Ok(Self::Paused),
            0x04 => Ok(Self::QuarterEnd),
            0x05 => Ok(Self::Final),
            _ => Err(""),
        }
    }
}
