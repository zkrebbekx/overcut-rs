//! Identifier value objects.
//!
//! Each newtype wraps a primitive so the compiler keeps a driver code
//! apart from an asset identifier, and a round apart from a gameday.

use std::fmt;

use crate::DomainError;

/// A driver's three-letter code, such as `VER`. The code is the join key
/// between the classification source and the fantasy feed.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Tla(String);

impl Tla {
    /// Parses a code. The input is trimmed and upper-cased. Between one and
    /// four ASCII letters or digits are accepted.
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        let code = raw.trim().to_ascii_uppercase();
        let valid =
            (1..=4).contains(&code.len()) && code.bytes().all(|b| b.is_ascii_alphanumeric());
        if valid {
            Ok(Self(code))
        } else {
            Err(DomainError::InvalidTla(raw.to_string()))
        }
    }

    /// Returns the code as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Tla {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The fantasy game's identifier of one asset (a driver or a constructor).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AssetId(String);

impl AssetId {
    /// Wraps a non-empty identifier.
    pub fn new(raw: impl Into<String>) -> Result<Self, DomainError> {
        let id = raw.into();
        if id.trim().is_empty() {
            return Err(DomainError::EmptyId("asset"));
        }
        Ok(Self(id))
    }

    /// Returns the identifier as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AssetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The fantasy game's identifier of one constructor team. A driver and the
/// constructor it drives for share a team identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TeamId(String);

impl TeamId {
    /// Wraps a non-empty identifier.
    pub fn new(raw: impl Into<String>) -> Result<Self, DomainError> {
        let id = raw.into();
        if id.trim().is_empty() {
            return Err(DomainError::EmptyId("team"));
        }
        Ok(Self(id))
    }

    /// Returns the identifier as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TeamId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A calendar round, numbered from 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RoundNumber(u32);

impl RoundNumber {
    /// Wraps a round number of at least 1.
    pub fn new(n: u32) -> Result<Self, DomainError> {
        if n == 0 {
            Err(DomainError::ZeroRound)
        } else {
            Ok(Self(n))
        }
    }

    /// Returns the number.
    pub fn get(self) -> u32 {
        self.0
    }

    /// Returns the round before this one, or `None` for round 1.
    pub fn previous(self) -> Option<Self> {
        (self.0 > 1).then(|| Self(self.0 - 1))
    }

    /// Returns the gameday that scores this round. The game numbers its
    /// gamedays like the calendar numbers its rounds.
    pub fn gameday(self) -> Gameday {
        Gameday(self.0)
    }
}

impl fmt::Display for RoundNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A fantasy gameday, numbered from 1. One gameday scores one round.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Gameday(u32);

impl Gameday {
    /// Wraps a gameday number of at least 1.
    pub fn new(n: u32) -> Result<Self, DomainError> {
        if n == 0 {
            Err(DomainError::ZeroGameday)
        } else {
            Ok(Self(n))
        }
    }

    /// Returns the number.
    pub fn get(self) -> u32 {
        self.0
    }

    /// Returns the gameday before this one, or `None` for gameday 1.
    pub fn previous(self) -> Option<Self> {
        (self.0 > 1).then(|| Self(self.0 - 1))
    }

    /// Returns the gameday after this one.
    #[must_use]
    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

impl fmt::Display for Gameday {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_a_lowercase_code_when_parsed_then_it_is_uppercased() {
        let tla = Tla::parse(" ver ").unwrap();
        assert_eq!(tla.as_str(), "VER");
    }

    #[test]
    fn given_an_empty_code_when_parsed_then_it_is_rejected() {
        assert_eq!(Tla::parse("  "), Err(DomainError::InvalidTla("  ".into())));
    }

    #[test]
    fn given_a_code_with_a_space_when_parsed_then_it_is_rejected() {
        assert!(Tla::parse("A B").is_err());
    }

    #[test]
    fn given_round_one_when_previous_is_asked_then_there_is_none() {
        assert_eq!(RoundNumber::new(1).unwrap().previous(), None);
        assert_eq!(
            RoundNumber::new(2).unwrap().previous(),
            RoundNumber::new(1).ok()
        );
    }

    #[test]
    fn given_zero_when_a_round_is_built_then_it_is_rejected() {
        assert_eq!(RoundNumber::new(0), Err(DomainError::ZeroRound));
        assert_eq!(Gameday::new(0), Err(DomainError::ZeroGameday));
    }
}
