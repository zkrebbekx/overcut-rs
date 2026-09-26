//! The optimizer's inputs and outputs.

use std::collections::HashSet;

use crate::shared::{AssetId, AssetKind};

/// One selectable asset with its projected points.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    /// The asset.
    pub id: AssetId,
    /// The display name.
    pub name: String,
    /// Driver or constructor.
    pub kind: AssetKind,
    /// The price in millions.
    pub price: f64,
    /// The projected points for the scoring round.
    pub points: f64,
}

/// The required team shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeamShape {
    /// The driver count.
    pub drivers: usize,
    /// The constructor count.
    pub constructors: usize,
}

impl TeamShape {
    /// The total slot count.
    pub fn slots(self) -> usize {
        self.drivers + self.constructors
    }
}

/// The controls of one optimization run.
#[derive(Debug, Clone, PartialEq)]
pub struct OptimizerOptions {
    /// The spending cap in millions. Ignored with the Limitless chip.
    pub budget: f64,
    /// The team shape.
    pub shape: TeamShape,
    /// The regular Boost on the best driver.
    pub boost_multiplier: u32,
    /// Apply the x3 chip: the best driver scores triple and the regular
    /// Boost moves to the second-best driver.
    pub extra_boost: bool,
    /// The x3 chip multiplier.
    pub extra_boost_multiplier: u32,
    /// The assets of the team now. When set, the optimizer charges the
    /// transfer penalty for changes beyond the free allowance.
    pub current_team: HashSet<AssetId>,
    /// The free transfer allowance. Ignored when `current_team` is empty.
    pub free_transfers: u32,
    /// The points cost per extra transfer (a negative number).
    pub transfer_penalty: i32,
    /// Remove the budget cap for this run.
    pub limitless: bool,
    /// Remove the transfer penalty for this run.
    pub wildcard: bool,
    /// The result count.
    pub top_n: usize,
}

impl OptimizerOptions {
    /// Builds options for a fresh team with no transfer cost and default
    /// multipliers.
    pub fn new(budget: f64, shape: TeamShape) -> Self {
        Self {
            budget,
            shape,
            boost_multiplier: 2,
            extra_boost: false,
            extra_boost_multiplier: 3,
            current_team: HashSet::new(),
            free_transfers: 0,
            transfer_penalty: 0,
            limitless: false,
            wildcard: false,
            top_n: 1,
        }
    }

    /// Sets the result count.
    #[must_use]
    pub fn top(mut self, n: usize) -> Self {
        self.top_n = n.max(1);
        self
    }

    /// Sets the current team and the transfer economics.
    #[must_use]
    pub fn with_current_team(
        mut self,
        team: impl IntoIterator<Item = AssetId>,
        free: u32,
        penalty: i32,
    ) -> Self {
        self.current_team = team.into_iter().collect();
        self.free_transfers = free;
        self.transfer_penalty = penalty;
        self
    }
}

/// One scored team.
#[derive(Debug, Clone, PartialEq)]
pub struct Lineup {
    /// The drivers, in candidate order.
    pub drivers: Vec<Candidate>,
    /// The constructors, in candidate order.
    pub constructors: Vec<Candidate>,
    /// The driver with the highest multiplier.
    pub captain_id: AssetId,
    /// The driver with the regular Boost when the x3 chip is played.
    pub boost_id: Option<AssetId>,
    /// The total price in millions.
    pub cost: f64,
    /// The sum of asset points without boosts or penalty.
    pub raw_points: f64,
    /// The extra points from every boost multiplier.
    pub captain_points: f64,
    /// The changes from the current team.
    pub transfers: u32,
    /// The transfer penalty (zero or negative).
    pub penalty: f64,
    /// `raw_points + captain_points + penalty`.
    pub score: f64,
}

impl Lineup {
    /// Every asset of the lineup, drivers first.
    pub fn assets(&self) -> impl Iterator<Item = &Candidate> {
        self.drivers.iter().chain(self.constructors.iter())
    }

    /// The identifiers of every asset in the lineup.
    pub fn ids(&self) -> HashSet<AssetId> {
        self.assets().map(|a| a.id.clone()).collect()
    }

    /// The multiplier a driver scores with: the x3 chip on the captain
    /// when a Boost holder exists, otherwise the regular Boost on the
    /// captain, the regular Boost on the Boost holder, and one elsewhere.
    pub fn multiplier(&self, id: &AssetId) -> f64 {
        if *id == self.captain_id {
            if self.boost_id.is_some() {
                3.0
            } else {
                2.0
            }
        } else if self.boost_id.as_ref() == Some(id) {
            2.0
        } else {
            1.0
        }
    }
}
