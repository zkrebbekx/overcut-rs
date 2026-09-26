//! The enumeration.

use crate::shared::AssetKind;

use super::{Candidate, Combinations, Lineup, OptimizerOptions};

/// Per-subset aggregates, computed once per driver set and once per
/// constructor pair so the cross product stays a handful of additions.
struct Aggregate {
    idx: Vec<usize>,
    cost: f64,
    points: f64,
    best: f64,
    second: f64,
    best_idx: Option<usize>,
    second_idx: Option<usize>,
    keepers: u32,
}

fn aggregate(set: Vec<usize>, pool: &[Candidate], opt: &OptimizerOptions) -> Aggregate {
    let mut a = Aggregate {
        idx: set,
        cost: 0.0,
        points: 0.0,
        best: 0.0,
        second: 0.0,
        best_idx: None,
        second_idx: None,
        keepers: 0,
    };
    for &i in &a.idx {
        let c = &pool[i];
        a.cost += c.price;
        a.points += c.points;
        if a.best_idx.is_none() || c.points > a.best {
            a.second = a.best;
            a.second_idx = a.best_idx;
            a.best = c.points;
            a.best_idx = Some(i);
        } else if a.second_idx.is_none() || c.points > a.second {
            a.second = c.points;
            a.second_idx = Some(i);
        }
        if opt.current_team.contains(&c.id) {
            a.keepers += 1;
        }
    }
    a
}

/// Inserts a lineup into a descending top-N list.
fn insert_top(top: &mut Vec<Lineup>, lineup: Lineup, n: usize) {
    let at = top.partition_point(|t| t.score >= lineup.score);
    top.insert(at, lineup);
    top.truncate(n);
}

/// Enumerates every legal team and returns the `top_n` best by score,
/// descending. Returns an empty list when the pool cannot fill the shape
/// or no team fits the budget.
pub fn best_lineups(candidates: &[Candidate], opt: &OptimizerOptions) -> Vec<Lineup> {
    let top_n = opt.top_n.max(1);
    let drivers: Vec<Candidate> = candidates
        .iter()
        .filter(|c| c.kind == AssetKind::Driver)
        .cloned()
        .collect();
    let cons: Vec<Candidate> = candidates
        .iter()
        .filter(|c| c.kind == AssetKind::Constructor)
        .cloned()
        .collect();
    if drivers.len() < opt.shape.drivers || cons.len() < opt.shape.constructors {
        return Vec::new();
    }

    let driver_aggs: Vec<Aggregate> = Combinations::new(drivers.len(), opt.shape.drivers)
        .map(|set| aggregate(set, &drivers, opt))
        .collect();
    let cons_aggs: Vec<Aggregate> = Combinations::new(cons.len(), opt.shape.constructors)
        .map(|set| aggregate(set, &cons, opt))
        .collect();

    let slots = opt.shape.slots() as u32;
    let boost_x = f64::from(opt.boost_multiplier.max(1) - 1);
    let extra_x = f64::from(opt.extra_boost_multiplier.max(1) - 1);
    let has_current = !opt.current_team.is_empty();

    let mut top: Vec<Lineup> = Vec::with_capacity(top_n + 1);
    let mut worst = f64::NEG_INFINITY;
    for da in &driver_aggs {
        for ca in &cons_aggs {
            let cost = da.cost + ca.cost;
            if !opt.limitless && cost > opt.budget + 1e-9 {
                continue;
            }
            let raw = da.points + ca.points;
            // The x3 chip triples the best driver; the regular Boost then
            // doubles the second-best. Without the chip, the Boost doubles
            // the best.
            let captain = if opt.extra_boost {
                extra_x * da.best + boost_x * da.second
            } else {
                boost_x * da.best
            };
            let (mut transfers, mut penalty) = (0, 0.0);
            if has_current {
                transfers = slots - da.keepers - ca.keepers;
                if !opt.wildcard && transfers > opt.free_transfers {
                    penalty =
                        f64::from((transfers - opt.free_transfers) as i32 * opt.transfer_penalty);
                }
            }
            let score = raw + captain + penalty;
            if top.len() == top_n && score <= worst {
                continue;
            }
            let Some(best_idx) = da.best_idx else {
                continue;
            };
            let lineup = Lineup {
                drivers: da.idx.iter().map(|&i| drivers[i].clone()).collect(),
                constructors: ca.idx.iter().map(|&i| cons[i].clone()).collect(),
                captain_id: drivers[best_idx].id.clone(),
                boost_id: if opt.extra_boost {
                    da.second_idx.map(|i| drivers[i].id.clone())
                } else {
                    None
                },
                cost,
                raw_points: raw,
                captain_points: captain,
                transfers,
                penalty,
                score,
            };
            insert_top(&mut top, lineup, top_n);
            worst = top.last().map_or(f64::NEG_INFINITY, |t| t.score);
        }
    }
    top
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::optimizer::TeamShape;
    use crate::shared::AssetId;

    fn id(s: &str) -> AssetId {
        AssetId::new(s).unwrap()
    }

    /// A synthetic pool: `nd` drivers and `nc` constructors with rising
    /// prices and points.
    fn field(nd: usize, nc: usize) -> Vec<Candidate> {
        let mut out = Vec::new();
        for i in 0..nd {
            out.push(Candidate {
                id: id(&format!("d{i}")),
                name: format!("D{i}"),
                kind: AssetKind::Driver,
                price: 5.0 + i as f64,
                points: (10 + i * 2) as f64,
            });
        }
        for i in 0..nc {
            out.push(Candidate {
                id: id(&format!("c{i}")),
                name: format!("C{i}"),
                kind: AssetKind::Constructor,
                price: 10.0 + i as f64 * 2.0,
                points: (20 + i * 5) as f64,
            });
        }
        out
    }

    fn shape() -> TeamShape {
        TeamShape {
            drivers: 5,
            constructors: 2,
        }
    }

    /// Checks every team with an independent bitmask enumeration and
    /// returns the best score.
    fn brute_force(assets: &[Candidate], budget: f64) -> f64 {
        let drivers: Vec<&Candidate> = assets
            .iter()
            .filter(|a| a.kind == AssetKind::Driver)
            .collect();
        let cons: Vec<&Candidate> = assets
            .iter()
            .filter(|a| a.kind == AssetKind::Constructor)
            .collect();
        let mut best = f64::NEG_INFINITY;
        for mask in 0u32..(1 << drivers.len()) {
            if mask.count_ones() != 5 {
                continue;
            }
            for cm in 0u32..(1 << cons.len()) {
                if cm.count_ones() != 2 {
                    continue;
                }
                let (mut cost, mut pts, mut top) = (0.0, 0.0, f64::NEG_INFINITY);
                for (i, d) in drivers.iter().enumerate() {
                    if mask & (1 << i) != 0 {
                        cost += d.price;
                        pts += d.points;
                        top = top.max(d.points);
                    }
                }
                for (i, c) in cons.iter().enumerate() {
                    if cm & (1 << i) != 0 {
                        cost += c.price;
                        pts += c.points;
                    }
                }
                if cost <= budget {
                    best = best.max(pts + top);
                }
            }
        }
        best
    }

    mod given_a_field_of_eight_drivers_and_four_constructors {
        use super::*;

        #[test]
        fn when_the_budget_covers_every_asset_then_the_best_five_and_two_are_picked_with_the_boost_on_the_best(
        ) {
            let teams = best_lineups(&field(8, 4), &OptimizerOptions::new(1000.0, shape()));
            assert_eq!(teams.len(), 1);
            let t = &teams[0];
            assert_eq!(t.raw_points, f64::from(16 + 18 + 20 + 22 + 24 + 30 + 35));
            assert_eq!(t.captain_id, id("d7"));
            assert_eq!(t.boost_id, None);
            assert_eq!(t.captain_points, 24.0);
            assert_eq!(t.score, t.raw_points + 24.0);
        }

        #[test]
        fn when_the_budget_forces_a_compromise_then_the_team_fits_and_matches_brute_force() {
            let assets = field(8, 4);
            let teams = best_lineups(&assets, &OptimizerOptions::new(60.0, shape()));
            assert_eq!(teams.len(), 1);
            assert!(teams[0].cost <= 60.0);
            assert_eq!(teams[0].score, brute_force(&assets, 60.0));
        }

        #[test]
        fn when_the_current_team_differs_and_two_transfers_are_free_then_the_extra_transfers_are_charged(
        ) {
            let current = ["d0", "d1", "d2", "d3", "d4", "c0", "c1"].map(id);
            let opt = OptimizerOptions::new(1000.0, shape()).with_current_team(current, 2, -10);
            let t = &best_lineups(&field(8, 4), &opt)[0];
            assert!(t.transfers > 0);
            assert_eq!(t.penalty, f64::from(-10 * (t.transfers as i32 - 2)));
        }

        #[test]
        fn when_the_wildcard_is_set_then_no_penalty_applies() {
            let current = ["d0", "d1", "d2", "d3", "d4", "c0", "c1"].map(id);
            let mut opt = OptimizerOptions::new(1000.0, shape()).with_current_team(current, 2, -10);
            opt.wildcard = true;
            assert_eq!(best_lineups(&field(8, 4), &opt)[0].penalty, 0.0);
        }

        #[test]
        fn when_limitless_is_set_with_a_tiny_budget_then_the_budget_is_ignored() {
            let mut opt = OptimizerOptions::new(1.0, shape());
            opt.limitless = true;
            assert_eq!(
                best_lineups(&field(8, 4), &opt)[0].raw_points,
                f64::from(16 + 18 + 20 + 22 + 24 + 30 + 35)
            );
        }

        #[test]
        fn when_the_x3_chip_is_played_then_the_best_triples_and_the_second_best_doubles() {
            let mut opt = OptimizerOptions::new(1000.0, shape());
            opt.extra_boost = true;
            let t = &best_lineups(&field(8, 4), &opt)[0];
            assert_eq!(t.captain_points, f64::from(2 * 24 + 22));
            assert_eq!(t.captain_id, id("d7"));
            assert_eq!(t.boost_id, Some(id("d6")));
            assert_eq!(t.multiplier(&id("d7")), 3.0);
            assert_eq!(t.multiplier(&id("d6")), 2.0);
            assert_eq!(t.multiplier(&id("d5")), 1.0);
        }

        #[test]
        fn when_five_teams_are_requested_then_the_scores_descend() {
            let teams = best_lineups(&field(8, 4), &OptimizerOptions::new(1000.0, shape()).top(5));
            assert_eq!(teams.len(), 5);
            for w in teams.windows(2) {
                assert!(w[1].score <= w[0].score);
            }
        }

        #[test]
        fn when_the_pool_cannot_fill_the_shape_then_no_team_is_returned() {
            assert!(best_lineups(&field(4, 1), &OptimizerOptions::new(1000.0, shape())).is_empty());
        }
    }
}
