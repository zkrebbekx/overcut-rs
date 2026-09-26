//! The infrastructure layer of overcut: the adapters that connect the
//! application to the outside world.
//!
//! Each adapter implements one port. The port owns the record types; the
//! adapter resolves every quirk of its source before a record crosses the
//! boundary.
//!
//! | Module | Port | Source |
//! |---|---|---|
//! | [`jolpica`] | `RaceDataGateway` | The Jolpica (Ergast) race-data API |
//! | [`fantasy_feed`] | `FantasyFeedGateway` | The public F1 Fantasy feed |
//! | [`f1site`] | `StartingGridGateway` | The official Formula 1 results pages |
//! | [`file_store`] | `SeasonRepository` | One JSON file on disk |
//! | [`rules_file`] | (a loader) | A JSON file with scoring-rule overrides |
//! | [`clock`] | `Clock` | The system clock |
//!
//! The JSON file format of [`file_store`] is the format of the original Go
//! toolkit. A data file that the Go toolkit wrote loads without change.

pub mod clock;
pub mod f1site;
pub mod fantasy_feed;
pub mod file_store;
pub mod jolpica;
pub mod rules_file;

pub use clock::SystemClock;
pub use f1site::OfficialSiteClient;
pub use fantasy_feed::FantasyFeedClient;
pub use file_store::JsonSeasonRepository;
pub use jolpica::JolpicaClient;
pub use rules_file::load_rules;

/// The User-Agent header every HTTP adapter sends. The fantasy feed's CDN
/// rejects a request without one with HTTP 403.
pub(crate) const USER_AGENT: &str = concat!(
    "overcut-rs/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/zkrebbekx/overcut-rs)"
);
