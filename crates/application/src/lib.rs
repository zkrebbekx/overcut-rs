//! The application layer of overcut: use cases, ports, and the data
//! transfer objects that cross the boundary.
//!
//! A use case is one operation a user can ask for: project a round,
//! optimize a team, review a finished round, predict prices, run the
//! backtest, find the hindsight optimum, or sync the season. Each use case
//! takes plain input, calls the domain, and returns a view: a serialisable
//! record shaped for a client. The HTTP API and the CLI both call the same
//! use cases.
//!
//! The layer depends on the domain and on nothing else in the workspace.
//! It reaches the outside world through [`ports`]: traits that an
//! infrastructure adapter implements. The season store port lives in the
//! domain because it persists an aggregate; the data-source gateways live
//! here because only the sync use case needs them.

pub mod dto;
pub mod error;
pub mod ports;
pub mod usecases;

pub use error::AppError;
pub use usecases::context::AnalysisContext;
