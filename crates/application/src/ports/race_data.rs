//! The classification source: the calendar and every session result.

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use overcut_domain::season::Session;
use overcut_domain::shared::Tla;

use super::GatewayError;

/// One round on the calendar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarEntry {
    /// The round number.
    pub round: u32,
    /// The race name.
    pub name: String,
    /// The circuit identifier.
    pub circuit_id: String,
    /// The race date.
    pub date: Option<NaiveDate>,
    /// The weekend has a sprint.
    pub has_sprint: bool,
    /// The scheduled start of each session with a known time, in UTC.
    pub sessions: Vec<(Session, DateTime<Utc>)>,
}

/// One qualifying classification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualifyingClassification {
    /// The round number.
    pub round: u32,
    /// The final position per driver.
    pub positions: Vec<(Tla, u32)>,
}

/// One car in a race or sprint classification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassifiedCar {
    /// The driver.
    pub tla: Tla,
    /// The grid slot. Zero means a pit-lane start.
    pub grid: u32,
    /// The position in the classification.
    pub position: u32,
    /// The car was classified. A retired, disqualified, or withdrawn car
    /// is not classified.
    pub classified: bool,
    /// The car set the fastest lap.
    pub fastest_lap: bool,
}

/// One race or sprint classification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaceClassification {
    /// The round number.
    pub round: u32,
    /// Every car.
    pub cars: Vec<ClassifiedCar>,
}

/// Reads the season calendar and every classification.
#[async_trait]
pub trait RaceDataGateway: Send + Sync {
    /// The calendar for one season.
    async fn calendar(&self, season: u16) -> Result<Vec<CalendarEntry>, GatewayError>;

    /// Every qualifying classification of the season so far.
    async fn qualifying(&self, season: u16) -> Result<Vec<QualifyingClassification>, GatewayError>;

    /// Every sprint classification of the season so far.
    async fn sprints(&self, season: u16) -> Result<Vec<RaceClassification>, GatewayError>;

    /// Every race classification of the season so far.
    async fn races(&self, season: u16) -> Result<Vec<RaceClassification>, GatewayError>;
}
