//! The scoring functions.

use super::{Breakdown, DriverWeekend, ScoringRules};

/// A constructor's weekend points under normal scoring and under the No
/// Negative chip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConstructorScore {
    /// The points as scored normally.
    pub total: i32,
    /// The points with every negative category floored at zero.
    pub no_negative: i32,
}

/// Returns the table value for a 1-based position, or zero outside the
/// table.
fn positional(table: &[i32], pos: u32) -> i32 {
    if pos == 0 {
        return 0;
    }
    table.get(pos as usize - 1).copied().unwrap_or(0)
}

impl ScoringRules {
    /// Computes one driver's fantasy points for one weekend by category.
    pub fn driver_breakdown(&self, w: DriverWeekend) -> Breakdown {
        // Qualifying.
        let quali = if w.quali_pos == 0 || w.quali_dsq {
            self.quali_no_time
        } else {
            positional(&self.quali_points, w.quali_pos)
        };
        let mut b = Breakdown {
            quali,
            ..Breakdown::default()
        };

        // Sprint. Sprint qualifying scores nothing. Overtakes score even
        // when the driver retires. An unclassified driver takes the penalty
        // and no position points.
        if w.has_sprint {
            b.sprint_overtakes = w.sprint_overtakes as i32 * self.overtake;
            if w.sprint_dnf || w.sprint_dsq {
                b.sprint_dnf = self.sprint_dnf;
            } else {
                b.sprint_result = positional(&self.sprint_points, w.sprint_pos);
                let delta = w.sprint_grid as i32 - w.sprint_pos as i32;
                b.sprint_position = if delta > 0 {
                    delta * self.position_gained
                } else {
                    (-delta).min(self.sprint_max_lost) * self.position_lost
                };
                if w.sprint_fastest_lap {
                    b.sprint_fastest_lap = self.sprint_fastest_lap;
                }
            }
        }

        // Race. Same structure; positions lost are not capped.
        b.overtakes = w.overtakes as i32 * self.overtake;
        if w.dnf || w.race_dsq {
            b.race_dnf = self.race_dnf;
        } else {
            b.race_result = positional(&self.race_points, w.finish_pos);
            let delta = w.grid_pos as i32 - w.finish_pos as i32;
            b.race_position = if delta > 0 {
                delta * self.position_gained
            } else {
                -delta * self.position_lost
            };
            if w.fastest_lap {
                b.fastest_lap = self.fastest_lap;
            }
            if w.dotd {
                b.dotd = self.driver_of_the_day;
            }
        }
        b
    }

    /// Computes one driver's fantasy points for one weekend.
    pub fn driver_points(&self, w: DriverWeekend) -> i32 {
        self.driver_breakdown(w).total()
    }

    /// Returns the points for a constructor's pit stop of the given
    /// duration in seconds.
    pub fn pit_stop_points_for(&self, seconds: f64) -> i32 {
        self.pit_stop_points
            .iter()
            .find(|band| seconds < band.under_seconds)
            .map_or(0, |band| band.points)
    }

    /// Computes the progression bonus from the count of the team's drivers
    /// that reached Q2 and Q3.
    pub fn constructor_quali_bonus_for(&self, in_q2: u32, in_q3: u32) -> i32 {
        let b = self.constructor_quali_bonus;
        match (in_q2, in_q3) {
            (_, q3) if q3 >= 2 => b.both_in_q3,
            (_, 1) => b.one_in_q3,
            (q2, _) if q2 >= 2 => b.both_in_q2,
            (1, _) => b.one_in_q2,
            _ => b.none_in_q2,
        }
    }

    /// Computes one constructor's fantasy points for one weekend: the
    /// combined total of its two drivers (without the driver-of-the-day
    /// bonus), the qualifying progression bonus, the extra disqualification
    /// penalties, plus a pit-stop component.
    ///
    /// The pit-stop component is not derivable from public timing data. The
    /// caller supplies it; the projection model estimates it from the
    /// residual between official constructor points and rule-derived
    /// points.
    pub fn constructor_points(&self, a: DriverWeekend, b: DriverWeekend, pit_stop_pts: i32) -> i32 {
        self.constructor_score(a, b, pit_stop_pts).total
    }

    /// Computes a constructor's points twice: as scored normally, and as
    /// the No Negative chip scores them, with every negative category
    /// floored at zero (each driver's categories, the qualifying bonus, and
    /// the disqualification penalties).
    pub fn constructor_score(
        &self,
        a: DriverWeekend,
        b: DriverWeekend,
        pit_stop_pts: i32,
    ) -> ConstructorScore {
        let ba = self.driver_breakdown(a.without_driver_only_awards());
        let bb = self.driver_breakdown(b.without_driver_only_awards());
        let mut total = ba.total() + bb.total();
        let mut no_negative = ba.no_negative() + bb.no_negative();

        let (mut in_q2, mut in_q3) = (0, 0);
        for w in [a, b] {
            if w.quali_pos >= 1 && w.quali_pos <= self.q2_cutoff {
                in_q2 += 1;
            }
            if w.quali_pos >= 1 && w.quali_pos <= self.q3_cutoff {
                in_q3 += 1;
            }
            if w.quali_dsq {
                total += self.constructor_dsq.quali;
            }
            if w.has_sprint && w.sprint_dsq {
                total += self.constructor_dsq.sprint;
            }
            if w.race_dsq {
                total += self.constructor_dsq.race;
            }
        }
        let bonus = self.constructor_quali_bonus_for(in_q2, in_q3);
        total += bonus + pit_stop_pts;
        no_negative += bonus.max(0) + pit_stop_pts.max(0);
        ConstructorScore { total, no_negative }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> ScoringRules {
        ScoringRules::default()
    }

    mod given_the_default_2026_rules {
        use super::*;

        #[test]
        fn when_pole_wins_with_fastest_lap_and_dotd_then_the_driver_scores_55() {
            let pts = rules().driver_points(DriverWeekend {
                quali_pos: 1,
                grid_pos: 1,
                finish_pos: 1,
                fastest_lap: true,
                dotd: true,
                ..Default::default()
            });
            assert_eq!(pts, 55);
        }

        #[test]
        fn when_p15_gains_five_places_with_three_overtakes_then_the_driver_scores_9() {
            let pts = rules().driver_points(DriverWeekend {
                quali_pos: 15,
                grid_pos: 15,
                finish_pos: 10,
                overtakes: 3,
                ..Default::default()
            });
            assert_eq!(pts, 9);
        }

        #[test]
        fn when_p2_retires_then_the_driver_scores_minus_11() {
            let pts = rules().driver_points(DriverWeekend {
                quali_pos: 2,
                grid_pos: 2,
                dnf: true,
                ..Default::default()
            });
            assert_eq!(pts, -11);
        }

        #[test]
        fn when_three_overtakes_precede_a_retirement_then_the_overtakes_still_score() {
            let pts = rules().driver_points(DriverWeekend {
                quali_pos: 22,
                grid_pos: 22,
                finish_pos: 19,
                dnf: true,
                overtakes: 3,
                ..Default::default()
            });
            assert_eq!(pts, -17);
        }

        #[test]
        fn when_disqualified_from_qualifying_then_the_penalty_applies_and_the_race_scores() {
            let pts = rules().driver_points(DriverWeekend {
                quali_pos: 1,
                quali_dsq: true,
                grid_pos: 22,
                finish_pos: 12,
                ..Default::default()
            });
            assert_eq!(pts, -5 + 10);
        }

        #[test]
        fn when_no_quali_time_and_p12_from_p22_then_the_driver_scores_5() {
            let pts = rules().driver_points(DriverWeekend {
                quali_pos: 0,
                grid_pos: 22,
                finish_pos: 12,
                ..Default::default()
            });
            assert_eq!(pts, 5);
        }

        #[test]
        fn when_four_places_are_lost_from_p3_then_the_driver_scores_10() {
            let pts = rules().driver_points(DriverWeekend {
                quali_pos: 3,
                grid_pos: 3,
                finish_pos: 7,
                ..Default::default()
            });
            assert_eq!(pts, 10);
        }

        #[test]
        fn when_the_sprint_is_won_from_p2_with_fastest_lap_then_the_sprint_adds_14() {
            let pts = rules().driver_points(DriverWeekend {
                quali_pos: 1,
                grid_pos: 1,
                finish_pos: 1,
                has_sprint: true,
                sprint_grid: 2,
                sprint_pos: 1,
                sprint_fastest_lap: true,
                ..Default::default()
            });
            assert_eq!(pts, 35 + 14);
        }

        #[test]
        fn when_the_driver_retires_from_the_sprint_after_two_overtakes_then_the_sprint_scores_minus_8(
        ) {
            let pts = rules().driver_points(DriverWeekend {
                quali_pos: 5,
                grid_pos: 5,
                finish_pos: 5,
                has_sprint: true,
                sprint_grid: 5,
                sprint_dnf: true,
                sprint_overtakes: 2,
                ..Default::default()
            });
            assert_eq!(pts, -10 + 2 + 6 + 10);
        }

        #[test]
        fn when_fourteen_places_are_lost_in_the_sprint_then_the_loss_caps_at_10() {
            let pts = rules().driver_points(DriverWeekend {
                quali_pos: 1,
                grid_pos: 1,
                finish_pos: 1,
                has_sprint: true,
                sprint_grid: 1,
                sprint_pos: 15,
                ..Default::default()
            });
            assert_eq!(pts, 35 - 10);
        }

        #[test]
        fn when_fourteen_places_are_lost_in_the_race_then_the_loss_is_not_capped() {
            let pts = rules().driver_points(DriverWeekend {
                quali_pos: 1,
                grid_pos: 1,
                finish_pos: 15,
                ..Default::default()
            });
            assert_eq!(pts, -4);
        }
    }

    mod given_the_no_negative_chip {
        use super::*;

        #[test]
        fn when_p3_retires_after_two_overtakes_then_the_chip_keeps_10() {
            let b = rules().driver_breakdown(DriverWeekend {
                quali_pos: 3,
                grid_pos: 3,
                dnf: true,
                overtakes: 2,
                ..Default::default()
            });
            assert_eq!(b.total(), -10);
            assert_eq!(b.no_negative(), 10);
        }

        #[test]
        fn when_p2_drops_to_p8_with_fastest_lap_then_only_positions_lost_is_floored() {
            let b = rules().driver_breakdown(DriverWeekend {
                quali_pos: 2,
                grid_pos: 2,
                finish_pos: 8,
                fastest_lap: true,
                ..Default::default()
            });
            assert_eq!(b.total(), 9 + 4 - 6 + 10);
            assert_eq!(b.no_negative(), 9 + 4 + 10);
        }

        #[test]
        fn when_both_drivers_fall_in_q1_and_one_retires_then_the_chip_floors_both() {
            let a = DriverWeekend {
                quali_pos: 18,
                grid_pos: 18,
                dnf: true,
                ..Default::default()
            };
            let b = DriverWeekend {
                quali_pos: 20,
                grid_pos: 20,
                finish_pos: 16,
                ..Default::default()
            };
            let s = rules().constructor_score(a, b, 5);
            assert_eq!(s.total, -20 + 4 - 1 + 5);
            assert_eq!(s.no_negative, 4 + 5);
        }
    }

    mod given_a_constructor {
        use super::*;

        #[test]
        fn when_both_reach_q3_and_finish_one_two_with_pit_points_then_the_bonus_is_10() {
            let a = DriverWeekend {
                quali_pos: 1,
                grid_pos: 1,
                finish_pos: 1,
                ..Default::default()
            };
            let b = DriverWeekend {
                quali_pos: 2,
                grid_pos: 2,
                finish_pos: 2,
                ..Default::default()
            };
            assert_eq!(
                rules().constructor_points(a, b, 4),
                (10 + 25) + (9 + 18) + 10 + 4
            );
        }

        #[test]
        fn when_a_driver_holds_dotd_then_the_constructor_does_not_receive_it() {
            let a = DriverWeekend {
                quali_pos: 1,
                grid_pos: 1,
                finish_pos: 1,
                dotd: true,
                ..Default::default()
            };
            let b = DriverWeekend {
                quali_pos: 2,
                grid_pos: 2,
                finish_pos: 2,
                ..Default::default()
            };
            assert_eq!(
                rules().constructor_points(a, b, 0),
                (10 + 25) + (9 + 18) + 10
            );
        }

        #[test]
        fn when_both_fall_in_q1_then_the_progression_penalty_applies() {
            let a = DriverWeekend {
                quali_pos: 17,
                grid_pos: 17,
                finish_pos: 15,
                ..Default::default()
            };
            let b = DriverWeekend {
                quali_pos: 20,
                grid_pos: 20,
                finish_pos: 18,
                ..Default::default()
            };
            assert_eq!(rules().constructor_points(a, b, 0), 2 + 2 - 1);
        }

        #[test]
        fn when_one_reaches_q3_then_the_bonus_is_the_one_in_q3_value() {
            assert_eq!(rules().constructor_quali_bonus_for(2, 1), 5);
        }

        #[test]
        fn when_a_driver_is_disqualified_from_the_race_then_the_constructor_takes_an_extra_20() {
            let a = DriverWeekend {
                quali_pos: 1,
                grid_pos: 1,
                finish_pos: 1,
                race_dsq: true,
                ..Default::default()
            };
            let b = DriverWeekend {
                quali_pos: 2,
                grid_pos: 2,
                finish_pos: 1,
                ..Default::default()
            };
            assert_eq!(
                rules().constructor_points(a, b, 0),
                (10 - 20) + (9 + 25 + 1) + 10 - 20
            );
        }

        #[test]
        fn when_the_pit_stop_table_is_queried_then_each_band_returns_the_official_points() {
            let r = rules();
            assert_eq!(r.pit_stop_points_for(1.95), 20);
            assert_eq!(r.pit_stop_points_for(2.10), 10);
            assert_eq!(r.pit_stop_points_for(2.30), 5);
            assert_eq!(r.pit_stop_points_for(2.75), 2);
            assert_eq!(r.pit_stop_points_for(3.10), 0);
        }
    }
}
