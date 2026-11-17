/// A regulation time is a playing timer  used in the basketball
/// indicating a set amount of time that a quarter must consist.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegulationTime {
    pub minutes: u8,
    pub seconds: u8,
    pub centiseconds: u8,
}

/// A shot clock is a countdown timer used in the basketball
/// indicating a set amount of time that a team may possess
/// the ball before attempting to score a goal.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShotClockTime {
    pub seconds: u8,
    pub centiseconds: u8,
}

impl RegulationTime {
    #[must_use]
    pub const fn from_millis(millis: u32) -> Self {
        let total_seconds = millis / 1000;
        let minutes = if total_seconds / 60 > u8::MAX as u32 {
            u8::MAX
        } else {
            (total_seconds / 60) as u8
        };
        let seconds = (total_seconds % 60) as u8;
        let centiseconds = ((millis % 1000) / 10) as u8;

        Self {
            minutes,
            seconds,
            centiseconds,
        }
    }
}

impl ShotClockTime {
    #[must_use]
    pub const fn from_millis(millis: u32) -> Self {
        let total_seconds = millis / 1000;
        let seconds = if total_seconds > u8::MAX as u32 {
            u8::MAX
        } else {
            total_seconds as u8
        };
        let centiseconds = ((millis % 1000) / 10) as u8;

        Self {
            seconds,
            centiseconds,
        }
    }
}
