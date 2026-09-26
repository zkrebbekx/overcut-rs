//! The scoring rules as JSON.
//!
//! The same shape serves two purposes: the API returns the active rules,
//! and a file with any subset of the fields overrides the defaults.

use overcut_domain::rules::{PitStopBand, QualiBonus, ScoringRules, SessionPenalty};
use serde::{Deserialize, Serialize};

/// One penalty per session type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionPenaltyDto {
    /// Qualifying.
    pub quali: i32,
    /// Sprint.
    pub sprint: i32,
    /// Race.
    pub race: i32,
}

/// One pit-stop band.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PitStopBandDto {
    /// The exclusive upper bound in seconds.
    pub under_seconds: f64,
    /// The points.
    pub points: i32,
}

/// The constructor qualifying bonuses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct QualiBonusDto {
    /// Neither driver in Q2.
    pub none_in_q2: i32,
    /// One driver in Q2.
    pub one_in_q2: i32,
    /// Both drivers in Q2.
    pub both_in_q2: i32,
    /// One driver in Q3.
    pub one_in_q3: i32,
    /// Both drivers in Q3.
    pub both_in_q3: i32,
}

/// The scoring rules. Every field is optional on input so a file can
/// override a subset; on output every field is present.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct RulesDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quali_points: Option<Vec<i32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quali_no_time: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub race_points: Option<Vec<i32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position_gained: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position_lost: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overtake: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fastest_lap: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub driver_of_the_day: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub race_dnf: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sprint_points: Option<Vec<i32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sprint_max_lost: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sprint_fastest_lap: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sprint_dnf: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constructor_dsq: Option<SessionPenaltyDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pit_stop_points: Option<Vec<PitStopBandDto>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fastest_pit_stop: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record_pit_stop: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record_pit_stop_time: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price_floor: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price_cap: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price_form_rounds: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constructor_quali_bonus: Option<QualiBonusDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub q2_cutoff: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub q3_cutoff: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transfer_penalty: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub free_transfers: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_carry_over: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boost_multiplier: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra_boost_multiplier: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team_drivers: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team_constructors: Option<usize>,
}

impl RulesDto {
    /// Overlays the set fields onto `base` and returns the result.
    pub fn apply_to(self, mut base: ScoringRules) -> ScoringRules {
        macro_rules! set {
            ($($field:ident),*) => { $( if let Some(v) = self.$field { base.$field = v; } )* };
        }
        set!(
            quali_points,
            quali_no_time,
            race_points,
            position_gained,
            position_lost,
            overtake,
            fastest_lap,
            driver_of_the_day,
            race_dnf,
            sprint_points,
            sprint_max_lost,
            sprint_fastest_lap,
            sprint_dnf,
            fastest_pit_stop,
            record_pit_stop,
            record_pit_stop_time,
            price_floor,
            price_cap,
            price_form_rounds,
            q2_cutoff,
            q3_cutoff,
            transfer_penalty,
            free_transfers,
            max_carry_over,
            boost_multiplier,
            extra_boost_multiplier,
            budget,
            team_drivers,
            team_constructors
        );
        if let Some(p) = self.constructor_dsq {
            base.constructor_dsq = SessionPenalty {
                quali: p.quali,
                sprint: p.sprint,
                race: p.race,
            };
        }
        if let Some(bands) = self.pit_stop_points {
            base.pit_stop_points = bands
                .into_iter()
                .map(|b| PitStopBand {
                    under_seconds: b.under_seconds,
                    points: b.points,
                })
                .collect();
        }
        if let Some(q) = self.constructor_quali_bonus {
            base.constructor_quali_bonus = QualiBonus {
                none_in_q2: q.none_in_q2,
                one_in_q2: q.one_in_q2,
                both_in_q2: q.both_in_q2,
                one_in_q3: q.one_in_q3,
                both_in_q3: q.both_in_q3,
            };
        }
        base
    }
}

impl From<&ScoringRules> for RulesDto {
    fn from(r: &ScoringRules) -> Self {
        Self {
            quali_points: Some(r.quali_points.clone()),
            quali_no_time: Some(r.quali_no_time),
            race_points: Some(r.race_points.clone()),
            position_gained: Some(r.position_gained),
            position_lost: Some(r.position_lost),
            overtake: Some(r.overtake),
            fastest_lap: Some(r.fastest_lap),
            driver_of_the_day: Some(r.driver_of_the_day),
            race_dnf: Some(r.race_dnf),
            sprint_points: Some(r.sprint_points.clone()),
            sprint_max_lost: Some(r.sprint_max_lost),
            sprint_fastest_lap: Some(r.sprint_fastest_lap),
            sprint_dnf: Some(r.sprint_dnf),
            constructor_dsq: Some(SessionPenaltyDto {
                quali: r.constructor_dsq.quali,
                sprint: r.constructor_dsq.sprint,
                race: r.constructor_dsq.race,
            }),
            pit_stop_points: Some(
                r.pit_stop_points
                    .iter()
                    .map(|b| PitStopBandDto {
                        under_seconds: b.under_seconds,
                        points: b.points,
                    })
                    .collect(),
            ),
            fastest_pit_stop: Some(r.fastest_pit_stop),
            record_pit_stop: Some(r.record_pit_stop),
            record_pit_stop_time: Some(r.record_pit_stop_time),
            price_floor: Some(r.price_floor),
            price_cap: Some(r.price_cap),
            price_form_rounds: Some(r.price_form_rounds),
            constructor_quali_bonus: Some(QualiBonusDto {
                none_in_q2: r.constructor_quali_bonus.none_in_q2,
                one_in_q2: r.constructor_quali_bonus.one_in_q2,
                both_in_q2: r.constructor_quali_bonus.both_in_q2,
                one_in_q3: r.constructor_quali_bonus.one_in_q3,
                both_in_q3: r.constructor_quali_bonus.both_in_q3,
            }),
            q2_cutoff: Some(r.q2_cutoff),
            q3_cutoff: Some(r.q3_cutoff),
            transfer_penalty: Some(r.transfer_penalty),
            free_transfers: Some(r.free_transfers),
            max_carry_over: Some(r.max_carry_over),
            boost_multiplier: Some(r.boost_multiplier),
            extra_boost_multiplier: Some(r.extra_boost_multiplier),
            budget: Some(r.budget),
            team_drivers: Some(r.team_drivers),
            team_constructors: Some(r.team_constructors),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_a_partial_override_when_applied_then_only_those_fields_change() {
        let dto: RulesDto = serde_json::from_str(r#"{"race_dnf": -15, "budget": 110.5}"#).unwrap();
        let rules = dto.apply_to(ScoringRules::default());
        assert_eq!(rules.race_dnf, -15);
        assert_eq!(rules.budget, 110.5);
        assert_eq!(rules.quali_no_time, -5);
    }

    #[test]
    fn given_the_default_rules_when_round_tripped_through_the_dto_then_they_are_unchanged() {
        let rules = ScoringRules::default();
        let json = serde_json::to_string(&RulesDto::from(&rules)).unwrap();
        let back: RulesDto = serde_json::from_str(&json).unwrap();
        assert_eq!(back.apply_to(ScoringRules::default()), rules);
    }
}
