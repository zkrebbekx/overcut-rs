//! What is already known about a weekend at simulation time.

use std::collections::{BTreeMap, BTreeSet};

use crate::shared::Tla;

/// The known weekend state. Every field is optional. Positions are
/// 1-based and keyed by driver code.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WeekendConditions {
    /// The actual qualifying classification. When set, the model does not
    /// sample qualifying.
    pub quali: BTreeMap<Tla, u32>,
    /// The actual starting grid after penalties. When unset, the grid
    /// follows the qualifying order with `back_of_grid` drivers moved to
    /// the back.
    pub grid: BTreeMap<Tla, u32>,
    /// Drivers that start from the back of the grid.
    pub back_of_grid: BTreeSet<Tla>,
    /// A practice classification, used as a pace prior when qualifying is
    /// unknown. Half the weight goes to practice and half to season form.
    pub practice: BTreeMap<Tla, u32>,
    /// The sprint grid, once the sprint has run.
    pub sprint_grid: BTreeMap<Tla, u32>,
    /// The sprint classification, once the sprint has run. When set the
    /// model does not sample the sprint leg.
    pub sprint_finish: BTreeMap<Tla, u32>,
    /// The cars that did not classify in the sprint.
    pub sprint_dnf: BTreeSet<Tla>,
}

impl WeekendConditions {
    /// Converts an order of codes from P1 into a position map.
    pub fn positions(order: &[Tla]) -> BTreeMap<Tla, u32> {
        order
            .iter()
            .enumerate()
            .map(|(i, tla)| (tla.clone(), i as u32 + 1))
            .collect()
    }

    /// Sets the sprint result from a classification with the retired cars
    /// last, the sprint grid, and the retired set. A retired car takes no
    /// classified position.
    #[must_use]
    pub fn with_sprint(mut self, finish: &[Tla], grid: &[Tla], dnf: BTreeSet<Tla>) -> Self {
        self.sprint_grid = Self::positions(grid);
        let mut pos = 1;
        self.sprint_finish = finish
            .iter()
            .filter(|tla| !dnf.contains(tla))
            .map(|tla| {
                let p = pos;
                pos += 1;
                (tla.clone(), p)
            })
            .collect();
        self.sprint_dnf = dnf;
        self
    }

    /// Reports whether the sprint result is set.
    pub fn has_sprint_result(&self) -> bool {
        !self.sprint_finish.is_empty()
    }
}
