//! The domain layer of overcut, an F1 Fantasy analysis toolkit.
//!
//! This crate holds the ubiquitous language of the game and the analysis
//! that runs on top of it. It has no I/O. It does not know about files,
//! HTTP, or a terminal. Every other crate depends on this one; this one
//! depends on nothing in the workspace.
//!
//! The bounded context has five parts:
//!
//! - [`rules`]: the official scoring tables and the scoring functions. The
//!   tables are a value object; the functions are pure.
//! - [`season`]: the `Season` aggregate. It holds the calendar, every
//!   classification, and every asset's per-gameday market snapshot. The
//!   repository port for the aggregate lives here too.
//! - [`projection`]: the pace model and the Monte Carlo simulation. It
//!   turns a season into a points distribution per asset.
//! - [`optimizer`]: the exact team enumerator.
//! - [`pricing`]: the price-change predictor.
//! - [`backtest`]: the walk-forward accuracy measurement.
//!
//! Types that need a valid state carry a constructor that returns a
//! [`DomainError`]. A caller cannot build an invalid value.

pub mod backtest;
pub mod error;
pub mod optimizer;
pub mod pricing;
pub mod projection;
pub mod rules;
pub mod season;
pub mod shared;

pub use error::DomainError;
