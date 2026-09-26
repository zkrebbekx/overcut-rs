//! One driver's weekend and its scored breakdown.

/// One driver's simulated or actual results for one race weekend. A
/// position is 1-based; zero means "no classified position".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DriverWeekend {
    /// The final qualifying classification. Zero means no time set.
    pub quali_pos: u32,
    /// Disqualified from qualifying.
    pub quali_dsq: bool,
    /// The official starting grid slot.
    pub grid_pos: u32,
    /// The race classification. Ignored when `dnf` is set.
    pub finish_pos: u32,
    /// DNF, NC, or DSQ in the race.
    pub dnf: bool,
    /// Disqualified from the race (a DNF for the driver, plus a
    /// constructor penalty).
    pub race_dsq: bool,
    /// On-track overtakes in the race. They score even when the driver
    /// retires later.
    pub overtakes: u32,
    /// The driver set the fastest race lap.
    pub fastest_lap: bool,
    /// The driver won the driver-of-the-day vote.
    pub dotd: bool,

    /// The weekend has a sprint. The sprint fields below apply only then.
    pub has_sprint: bool,
    /// The sprint grid slot.
    pub sprint_grid: u32,
    /// The sprint classification. Ignored when `sprint_dnf` is set.
    pub sprint_pos: u32,
    /// Did not classify in the sprint.
    pub sprint_dnf: bool,
    /// Disqualified from the sprint.
    pub sprint_dsq: bool,
    /// On-track overtakes in the sprint.
    pub sprint_overtakes: u32,
    /// The driver set the fastest sprint lap.
    pub sprint_fastest_lap: bool,
}

impl DriverWeekend {
    /// Returns the weekend without the awards that only a driver scores.
    /// A constructor does not receive the driver-of-the-day bonus.
    pub(crate) fn without_driver_only_awards(mut self) -> Self {
        self.dotd = false;
        self
    }
}

/// One driver's weekend points by scoring category. The No Negative chip
/// floors each category at zero separately.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Breakdown {
    /// Qualifying position points, or the no-time penalty.
    pub quali: i32,
    /// Sprint finish points.
    pub sprint_result: i32,
    /// Sprint positions gained or lost.
    pub sprint_position: i32,
    /// Sprint overtake points.
    pub sprint_overtakes: i32,
    /// Sprint fastest-lap bonus.
    pub sprint_fastest_lap: i32,
    /// Sprint DNF penalty.
    pub sprint_dnf: i32,
    /// Race finish points.
    pub race_result: i32,
    /// Race positions gained or lost.
    pub race_position: i32,
    /// Race overtake points.
    pub overtakes: i32,
    /// Race fastest-lap bonus.
    pub fastest_lap: i32,
    /// Driver-of-the-day bonus.
    pub dotd: i32,
    /// Race DNF penalty.
    pub race_dnf: i32,
}

impl Breakdown {
    fn categories(&self) -> [i32; 12] {
        [
            self.quali,
            self.sprint_result,
            self.sprint_position,
            self.sprint_overtakes,
            self.sprint_fastest_lap,
            self.sprint_dnf,
            self.race_result,
            self.race_position,
            self.overtakes,
            self.fastest_lap,
            self.dotd,
            self.race_dnf,
        ]
    }

    /// Sums every category.
    pub fn total(&self) -> i32 {
        self.categories().iter().sum()
    }

    /// Sums every category with each negative category floored at zero,
    /// as the No Negative chip scores.
    pub fn no_negative(&self) -> i32 {
        self.categories().iter().map(|v| (*v).max(0)).sum()
    }
}
