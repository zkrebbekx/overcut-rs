//! The exact team optimizer.
//!
//! The search space is every legal team: C(22,5) driver sets times C(11,2)
//! constructor pairs, about 1.4 million teams. The optimizer scores every
//! one, so the result is the true optimum for the given projections, not
//! a heuristic.

mod combinations;
mod enumerate;
mod lineup;

pub use combinations::Combinations;
pub use enumerate::best_lineups;
pub use lineup::{Candidate, Lineup, OptimizerOptions, TeamShape};
