//! The two kinds of fantasy asset.

use std::fmt;

/// A fantasy asset is a driver or a constructor. A team holds five drivers
/// and two constructors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AssetKind {
    /// One driver.
    Driver,
    /// One constructor team.
    Constructor,
}

impl AssetKind {
    /// Returns the lower-case label the API uses.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Driver => "driver",
            Self::Constructor => "constructor",
        }
    }

    /// Both kinds, drivers first.
    pub const ALL: [Self; 2] = [Self::Driver, Self::Constructor];
}

impl fmt::Display for AssetKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
