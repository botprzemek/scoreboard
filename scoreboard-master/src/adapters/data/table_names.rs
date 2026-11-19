use std::fmt::{Display, Formatter, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Table {
    Games,
    Devices,
}

impl Table {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Games => "games",
            Self::Devices => "devices",
        }
    }
}

impl Display for Table {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_str(self.as_str())
    }
}
