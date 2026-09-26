//! Use case inputs.

use std::fmt;

use overcut_domain::DomainError;
use serde::{Deserialize, Serialize};

/// The known weekend state as a client sends it. Each order is a list of
/// driver codes from P1. Codes are trimmed and upper-cased on use.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConditionsInput {
    /// The qualifying order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quali: Vec<String>,
    /// The starting grid after penalties.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grid: Vec<String>,
    /// Drivers sent to the back of the grid.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub back: Vec<String>,
    /// A practice order used as a pace prior.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fp3: Vec<String>,
    /// The sprint classification with retired cars last.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sprint: Vec<String>,
    /// The sprint grid.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sprint_grid: Vec<String>,
    /// The cars that retired from the sprint.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sprint_dnf: Vec<String>,
}

impl ConditionsInput {
    /// Reports whether the caller supplied nothing.
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

/// The projection request. A missing round means the next round; zero
/// sims and seed take the defaults.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectInput {
    /// The round to project. Zero or absent means the next round.
    #[serde(default)]
    pub round: u32,
    /// The simulation count. Zero means the default.
    #[serde(default)]
    pub sims: usize,
    /// The random seed. Zero means the default.
    #[serde(default)]
    pub seed: u64,
    /// The known weekend state.
    #[serde(default)]
    pub conditions: ConditionsInput,
}

/// A chip the player can play for one round.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Chip {
    /// Unlimited transfers within the cost cap.
    #[serde(rename = "wildcard")]
    Wildcard,
    /// No cost cap and unlimited transfers for one round.
    #[serde(rename = "limitless")]
    Limitless,
    /// One driver scores triple; the regular Boost moves to another.
    #[serde(rename = "3x")]
    TripleBoost,
    /// Every negative scoring category is floored at zero.
    #[serde(rename = "nonegative")]
    NoNegative,
}

impl Chip {
    /// Parses the API label. An empty label means no chip.
    pub fn parse(label: &str) -> Result<Option<Self>, DomainError> {
        match label.trim() {
            "" => Ok(None),
            "wildcard" => Ok(Some(Self::Wildcard)),
            "limitless" => Ok(Some(Self::Limitless)),
            "3x" => Ok(Some(Self::TripleBoost)),
            "nonegative" => Ok(Some(Self::NoNegative)),
            other => Err(DomainError::UnknownChip(other.to_string())),
        }
    }

    /// The API label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Wildcard => "wildcard",
            Self::Limitless => "limitless",
            Self::TripleBoost => "3x",
            Self::NoNegative => "nonegative",
        }
    }
}

impl fmt::Display for Chip {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The projection quantile the optimizer maximises.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskMode {
    /// The expected value.
    #[default]
    Mean,
    /// The safe floor.
    P10,
    /// The ceiling.
    P90,
}

impl RiskMode {
    /// Parses the API label. An empty label means the mean.
    pub fn parse(label: &str) -> Result<Self, DomainError> {
        match label.trim() {
            "" | "mean" => Ok(Self::Mean),
            "p10" => Ok(Self::P10),
            "p90" => Ok(Self::P90),
            other => Err(DomainError::UnknownRiskMode(other.to_string())),
        }
    }

    /// The API label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mean => "mean",
            Self::P10 => "p10",
            Self::P90 => "p90",
        }
    }
}

impl fmt::Display for RiskMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The optimizer request.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OptimizeInput {
    /// The round. Zero means the next round.
    #[serde(default)]
    pub round: u32,
    /// The simulation count. Zero means the default.
    #[serde(default)]
    pub sims: usize,
    /// The random seed. Zero means the default.
    #[serde(default)]
    pub seed: u64,
    /// The current team as asset identifiers.
    #[serde(default)]
    pub team: Vec<String>,
    /// The free transfers available.
    #[serde(default)]
    pub free_transfers: u32,
    /// The budget in millions. Zero means the rules' budget.
    #[serde(default)]
    pub budget: f64,
    /// The chip label: empty, wildcard, limitless, 3x, or nonegative.
    #[serde(default)]
    pub chip: String,
    /// The risk mode label: mean, p10, or p90.
    #[serde(default)]
    pub risk: String,
    /// The count of teams to return. Zero means five.
    #[serde(default)]
    pub top: usize,
    /// The known weekend state.
    #[serde(default)]
    pub conditions: ConditionsInput,
}

/// The post-round review request. A zero round means the latest finished
/// round.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewInput {
    /// The round. Zero means the latest finished round.
    #[serde(default)]
    pub round: u32,
    /// The simulation count. Zero means the default.
    #[serde(default)]
    pub sims: usize,
    /// The random seed. Zero means the default.
    #[serde(default)]
    pub seed: u64,
    /// The asset identifiers the player held.
    #[serde(default)]
    pub team: Vec<String>,
    /// The driver that carried the Boost. Empty means the team's best
    /// projected driver.
    #[serde(default)]
    pub captain: String,
}
