#[non_exhaustive]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RegulationTime {
    pub minutes: u8,
    pub seconds: u8,
    pub centiseconds: u8,
}

#[non_exhaustive]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ClockTime {
    pub seconds: u8,
    pub centiseconds: u8,
}
