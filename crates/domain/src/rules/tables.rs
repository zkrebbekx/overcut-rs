//! The scoring tables.

/// Every scoring table and constant for one season.
#[derive(Debug, Clone, PartialEq)]
pub struct ScoringRules {
    /// Points per qualifying position, P1 first.
    pub quali_points: Vec<i32>,
    /// The penalty when a driver sets no time or is disqualified in
    /// qualifying.
    pub quali_no_time: i32,

    /// Points per race finish position, P1 first.
    pub race_points: Vec<i32>,
    /// Points per net position gained in the race.
    pub position_gained: i32,
    /// Points per net position lost in the race (a negative number).
    pub position_lost: i32,
    /// Points per on-track overtake.
    pub overtake: i32,
    /// The bonus for the fastest race lap.
    pub fastest_lap: i32,
    /// The bonus for the driver-of-the-day vote.
    pub driver_of_the_day: i32,
    /// The penalty for a DNF, NC, or DSQ in the race.
    pub race_dnf: i32,

    /// Points per sprint finish position, P1 first. Sprint qualifying
    /// scores nothing.
    pub sprint_points: Vec<i32>,
    /// The cap on positions lost in the sprint.
    pub sprint_max_lost: i32,
    /// The bonus for the fastest sprint lap.
    pub sprint_fastest_lap: i32,
    /// The penalty for a DNF in the sprint.
    pub sprint_dnf: i32,

    /// The extra constructor penalty per disqualified driver, by session.
    pub constructor_dsq: SessionPenalty,
    /// The pit-stop bands. The first band whose bound exceeds the stop
    /// time applies.
    pub pit_stop_points: Vec<PitStopBand>,
    /// The bonus for the fastest stop of the race.
    pub fastest_pit_stop: i32,
    /// The bonus for a new world-record stop.
    pub record_pit_stop: i32,
    /// The record to beat, in seconds.
    pub record_pit_stop_time: f64,

    /// The lowest price of any asset, in millions.
    pub price_floor: f64,
    /// The highest price of any asset, in millions.
    pub price_cap: f64,
    /// The count of grands prix whose average performance drives a price
    /// change.
    pub price_form_rounds: u32,

    /// The constructor qualifying progression bonuses.
    pub constructor_quali_bonus: QualiBonus,
    /// The highest qualifying position that reaches Q2. With 22 cars in
    /// 2026 the value is 16.
    pub q2_cutoff: u32,
    /// The highest qualifying position that reaches Q3.
    pub q3_cutoff: u32,

    /// The points cost of one transfer above the free allowance (a
    /// negative number).
    pub transfer_penalty: i32,
    /// The free transfer allowance per race week.
    pub free_transfers: u32,
    /// The most unused transfers that carry to the next race.
    pub max_carry_over: u32,
    /// The regular Boost multiplier on one driver, every race.
    pub boost_multiplier: u32,
    /// The x3 chip multiplier. The chip goes on a second driver; the
    /// regular Boost stays on another.
    pub extra_boost_multiplier: u32,
    /// The team budget in millions.
    pub budget: f64,
    /// The required driver count.
    pub team_drivers: usize,
    /// The required constructor count.
    pub team_constructors: usize,
}

/// One penalty value per session type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionPenalty {
    /// The qualifying penalty.
    pub quali: i32,
    /// The sprint penalty.
    pub sprint: i32,
    /// The race penalty.
    pub race: i32,
}

/// One row of the pit-stop table: a stop faster than `under_seconds`
/// scores `points`, unless a faster band applies.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PitStopBand {
    /// The exclusive upper bound of the band, in seconds.
    pub under_seconds: f64,
    /// The points for a stop inside the band.
    pub points: i32,
}

/// The constructor qualifying progression bonuses, keyed by the count of
/// the team's drivers that reach Q2 and Q3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QualiBonus {
    /// Neither driver reached Q2.
    pub none_in_q2: i32,
    /// One driver reached Q2.
    pub one_in_q2: i32,
    /// Both drivers reached Q2.
    pub both_in_q2: i32,
    /// One driver reached Q3.
    pub one_in_q3: i32,
    /// Both drivers reached Q3.
    pub both_in_q3: i32,
}

impl Default for ScoringRules {
    /// The 2026 scoring configuration, verified against the official rules
    /// page.
    fn default() -> Self {
        Self {
            quali_points: vec![10, 9, 8, 7, 6, 5, 4, 3, 2, 1],
            quali_no_time: -5,

            race_points: vec![25, 18, 15, 12, 10, 8, 6, 4, 2, 1],
            position_gained: 1,
            position_lost: -1,
            overtake: 1,
            fastest_lap: 10,
            driver_of_the_day: 10,
            race_dnf: -20,

            sprint_points: vec![8, 7, 6, 5, 4, 3, 2, 1],
            sprint_max_lost: 10,
            sprint_fastest_lap: 5,
            sprint_dnf: -10,

            constructor_dsq: SessionPenalty {
                quali: -5,
                sprint: -10,
                race: -20,
            },
            pit_stop_points: vec![
                PitStopBand {
                    under_seconds: 2.00,
                    points: 20,
                },
                PitStopBand {
                    under_seconds: 2.20,
                    points: 10,
                },
                PitStopBand {
                    under_seconds: 2.50,
                    points: 5,
                },
                PitStopBand {
                    under_seconds: 3.00,
                    points: 2,
                },
            ],
            fastest_pit_stop: 5,
            record_pit_stop: 15,
            record_pit_stop_time: 1.80,

            price_floor: 3.0,
            price_cap: 34.0,
            price_form_rounds: 3,

            constructor_quali_bonus: QualiBonus {
                none_in_q2: -1,
                one_in_q2: 1,
                both_in_q2: 3,
                one_in_q3: 5,
                both_in_q3: 10,
            },
            q2_cutoff: 16,
            q3_cutoff: 10,

            transfer_penalty: -10,
            free_transfers: 2,
            max_carry_over: 1,
            boost_multiplier: 2,
            extra_boost_multiplier: 3,
            budget: 100.0,
            team_drivers: 5,
            team_constructors: 2,
        }
    }
}
