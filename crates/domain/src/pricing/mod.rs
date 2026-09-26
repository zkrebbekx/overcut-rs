//! The price-change predictor.
//!
//! The game moves a price on the performance of the last three grands
//! prix, inside tier bounds. The predictor does not guess the game's exact
//! formula. It fits a linear model on the season's real price movements
//! (change versus the asset's recent-form z-score and ownership),
//! separately for drivers and constructors, then clips the prediction to
//! the observed tier bounds. [`backtest`] reports the walk-forward accuracy
//! on the same season, so the error is measured, not asserted.

use std::collections::BTreeSet;

use crate::season::{Asset, Season};
use crate::shared::{AssetId, AssetKind, Gameday};

/// The count of recent gamedays that drive a price change.
pub const FORM_WINDOW: usize = 3;

/// The price above which the game uses the low-volatility tier.
pub const TIER_SPLIT: f64 = 18.5;

/// The largest move per gameday in the expensive tier, in millions.
pub const TIER_A_MAX: f64 = 0.3;

/// The largest move per gameday in the cheap tier, in millions.
pub const TIER_B_MAX: f64 = 0.6;

/// One observed price movement with the features known before it.
#[derive(Debug, Clone, PartialEq)]
pub struct Example {
    /// The asset.
    pub asset_id: AssetId,
    /// Driver or constructor.
    pub kind: AssetKind,
    /// The gameday at which the change applied.
    pub gameday: Gameday,
    /// Price minus old price.
    pub change: f64,
    /// The recent-points z-score among assets of the same kind.
    pub form_z: f64,
    /// The ownership z-score among assets of the same kind.
    pub own_z: f64,
    /// The price before the change.
    pub price: f64,
}

/// One asset's predicted next change.
#[derive(Debug, Clone, PartialEq)]
pub struct Prediction {
    /// The asset.
    pub asset_id: AssetId,
    /// The display name.
    pub name: String,
    /// Driver or constructor.
    pub kind: AssetKind,
    /// The current price.
    pub price: f64,
    /// The predicted change, in millions.
    pub change: f64,
}

/// A fitted per-kind linear predictor: intercept, form, ownership.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PriceModel {
    driver: [f64; 3],
    constructor: [f64; 3],
}

/// The walk-forward accuracy of the predictor.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PriceBacktest {
    /// The count of predicted movements.
    pub examples: usize,
    /// The mean absolute error, in millions.
    pub mae: f64,
    /// The error of the always-zero prediction.
    pub naive_mae: f64,
    /// The sign hit rate on the moves that happened.
    pub direction: f64,
    /// The count of nonzero actual moves.
    pub moves: usize,
}

/// Bounds a change to the tier limits for the given price.
pub fn clip(change: f64, price: f64) -> f64 {
    let limit = if price > TIER_SPLIT {
        TIER_A_MAX
    } else {
        TIER_B_MAX
    };
    change.clamp(-limit, limit)
}

/// The mean official points of the last [`FORM_WINDOW`] gamedays at or
/// before `g`, or `None` with no history.
fn recent_form(a: &Asset, g: Gameday) -> Option<f64> {
    let pts: Vec<f64> = a
        .history
        .iter()
        .filter(|h| h.gameday <= g)
        .map(|h| h.points)
        .collect();
    if pts.is_empty() {
        return None;
    }
    let tail = &pts[pts.len().saturating_sub(FORM_WINDOW)..];
    Some(tail.iter().sum::<f64>() / tail.len() as f64)
}

fn moments(values: &[f64]) -> (f64, f64) {
    if values.is_empty() {
        return (0.0, 0.0);
    }
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    let var = values.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / n;
    (mean, var.sqrt())
}

/// Converts values to z-scores. A zero-variance set maps to zeros.
fn zscores(values: &[f64]) -> Vec<f64> {
    let (mean, sd) = moments(values);
    if sd == 0.0 {
        return vec![0.0; values.len()];
    }
    values.iter().map(|v| (v - mean) / sd).collect()
}

/// Extracts every observed price movement with features that were known
/// before the movement. The movement at gameday g uses form through
/// gameday g-1.
pub fn examples(season: &Season) -> Vec<Example> {
    let gamedays: BTreeSet<Gameday> = season
        .assets()
        .iter()
        .flat_map(|a| a.history.iter().map(|h| h.gameday))
        .collect();
    let mut out = Vec::new();
    for g in gamedays {
        let Some(prev_g) = g.previous() else { continue }; // no prior form
        for kind in AssetKind::ALL {
            let mut rows = Vec::new();
            let mut forms = Vec::new();
            let mut owns = Vec::new();
            for a in season.assets().iter().filter(|a| a.kind == kind) {
                let (Some(cur), Some(prev)) = (a.at(g), a.at(prev_g)) else {
                    continue;
                };
                let Some(form) = recent_form(a, prev_g) else {
                    continue;
                };
                rows.push(Example {
                    asset_id: a.id.clone(),
                    kind,
                    gameday: g,
                    change: cur.price_change(),
                    form_z: 0.0,
                    own_z: 0.0,
                    price: cur.old_price,
                });
                forms.push(form);
                owns.push(prev.ownership);
            }
            for ((row, fz), oz) in rows.iter_mut().zip(zscores(&forms)).zip(zscores(&owns)) {
                row.form_z = fz;
                row.own_z = oz;
            }
            out.extend(rows);
        }
    }
    out
}

/// Solves a three-parameter least squares by normal equations, with a
/// small ridge term on the feature coefficients. The ridge keeps the solve
/// stable when form and ownership correlate strongly.
fn ols(xs: &[[f64; 3]], ys: &[f64]) -> [f64; 3] {
    if xs.len() < 4 {
        return [0.0; 3];
    }
    let mut a = [[0.0; 4]; 3];
    for (x, y) in xs.iter().zip(ys) {
        for r in 0..3 {
            for c in 0..3 {
                a[r][c] += x[r] * x[c];
            }
            a[r][3] += x[r] * y;
        }
    }
    const RIDGE: f64 = 1e-3;
    a[1][1] += RIDGE;
    a[2][2] += RIDGE;
    // Gaussian elimination with partial pivoting.
    for col in 0..3 {
        let piv = (col..3)
            .max_by(|x, y| a[*x][col].abs().total_cmp(&a[*y][col].abs()))
            .unwrap_or(col);
        a.swap(col, piv);
        if a[col][col].abs() < 1e-12 {
            return [0.0; 3];
        }
        for r in 0..3 {
            if r == col {
                continue;
            }
            let f = a[r][col] / a[col][col];
            let pivot_row = a[col];
            for (cell, p) in a[r].iter_mut().zip(pivot_row.iter()).skip(col) {
                *cell -= f * p;
            }
        }
    }
    [a[0][3] / a[0][0], a[1][3] / a[1][1], a[2][3] / a[2][2]]
}

impl PriceModel {
    /// Computes per-kind least-squares coefficients on the examples.
    pub fn fit(examples: &[Example]) -> Self {
        let fit_kind = |kind: AssetKind| {
            let rows: Vec<&Example> = examples.iter().filter(|e| e.kind == kind).collect();
            let xs: Vec<[f64; 3]> = rows.iter().map(|e| [1.0, e.form_z, e.own_z]).collect();
            let ys: Vec<f64> = rows.iter().map(|e| e.change).collect();
            ols(&xs, &ys)
        };
        Self {
            driver: fit_kind(AssetKind::Driver),
            constructor: fit_kind(AssetKind::Constructor),
        }
    }

    /// The coefficients for one kind.
    pub fn coefficients(&self, kind: AssetKind) -> [f64; 3] {
        match kind {
            AssetKind::Driver => self.driver,
            AssetKind::Constructor => self.constructor,
        }
    }

    /// Applies the model to one feature row and clips to the tier.
    fn predict_one(&self, kind: AssetKind, form_z: f64, own_z: f64, price: f64) -> f64 {
        let c = self.coefficients(kind);
        clip(c[0] + c[1] * form_z + c[2] * own_z, price)
    }

    /// Predicts the next price change for every selectable asset from the
    /// latest gameday snapshot, largest rise first.
    pub fn predict(&self, season: &Season) -> Vec<Prediction> {
        let mut out = Vec::new();
        for kind in AssetKind::ALL {
            let mut assets = Vec::new();
            let mut forms = Vec::new();
            let mut owns = Vec::new();
            for a in season.selectable_assets().filter(|a| a.kind == kind) {
                let Some(latest) = a.latest() else { continue };
                let Some(form) = recent_form(a, latest.gameday) else {
                    continue;
                };
                assets.push((a, latest));
                forms.push(form);
                owns.push(latest.ownership);
            }
            for (((a, latest), fz), oz) in assets.iter().zip(zscores(&forms)).zip(zscores(&owns)) {
                out.push(Prediction {
                    asset_id: a.id.clone(),
                    name: a.name.clone(),
                    kind,
                    price: latest.price,
                    change: self.predict_one(kind, fz, oz, latest.price),
                });
            }
        }
        out.sort_by(|a, b| b.change.total_cmp(&a.change));
        out
    }
}

/// Fits on gamedays before g and predicts g, for every g with enough
/// history, and reports the pooled accuracy.
pub fn backtest(season: &Season) -> PriceBacktest {
    let all = examples(season);
    let mut rep = PriceBacktest::default();
    for e in all.iter().filter(|e| e.gameday.get() >= 4) {
        let train: Vec<Example> = all
            .iter()
            .filter(|t| t.gameday < e.gameday)
            .cloned()
            .collect();
        let pred = PriceModel::fit(&train).predict_one(e.kind, e.form_z, e.own_z, e.price);
        rep.examples += 1;
        rep.mae += (pred - e.change).abs();
        rep.naive_mae += e.change.abs();
        if e.change != 0.0 {
            rep.moves += 1;
            if pred != 0.0 && (pred > 0.0) == (e.change > 0.0) {
                rep.direction += 1.0;
            }
        }
    }
    if rep.examples > 0 {
        rep.mae /= rep.examples as f64;
        rep.naive_mae /= rep.examples as f64;
    }
    if rep.moves > 0 {
        rep.direction /= rep.moves as f64;
    }
    rep
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;
    use crate::season::GamedaySnapshot;
    use crate::shared::TeamId;

    /// A season where price changes track recent form perfectly: the asset
    /// with the best last-three points always gains.
    fn price_season() -> Season {
        let mut assets = Vec::new();
        for i in 0..6 {
            let mut p = 10.0;
            let history = (1..=8)
                .map(|g| {
                    let change = if g > 1 {
                        0.3 - f64::from(i) * 0.12
                    } else {
                        0.0
                    };
                    let old = p;
                    p += change;
                    GamedaySnapshot {
                        price: p,
                        old_price: old,
                        points: f64::from(30 - i * 5),
                        ownership: f64::from(60 - i * 10),
                        ..GamedaySnapshot::new(Gameday::new(g).unwrap(), p)
                    }
                })
                .collect();
            assets.push(Asset {
                id: AssetId::new(format!("d{i}")).unwrap(),
                kind: AssetKind::Driver,
                name: format!("D{i}"),
                tla: None,
                team_id: TeamId::new("t").unwrap(),
                team_name: "T".into(),
                history,
            });
        }
        Season::new(2026, Utc::now(), vec![], assets).unwrap()
    }

    mod given_the_tier_bounds {
        use super::*;

        #[test]
        fn when_a_cheap_asset_predicts_a_large_move_then_it_clips_to_the_low_tier_limit() {
            assert_eq!(clip(1.5, 10.0), TIER_B_MAX);
            assert_eq!(clip(-1.5, 10.0), -TIER_B_MAX);
        }

        #[test]
        fn when_an_expensive_asset_predicts_a_large_move_then_it_clips_to_the_high_tier_limit() {
            assert_eq!(clip(0.5, 25.0), TIER_A_MAX);
            assert_eq!(clip(-0.5, 25.0), -TIER_A_MAX);
        }

        #[test]
        fn when_the_prediction_sits_inside_the_bounds_then_it_passes_through() {
            assert_eq!(clip(0.1, 25.0), 0.1);
        }
    }

    mod given_a_season_where_form_drives_every_price_move {
        use super::*;

        #[test]
        fn when_the_examples_are_extracted_then_every_gameday_after_the_first_yields_one_per_asset()
        {
            let ex = examples(&price_season());
            assert_eq!(ex.len(), 6 * 7);
            for e in &ex {
                let i = f64::from(e.asset_id.as_str().as_bytes()[1] - b'0');
                assert!((e.change - (0.3 - i * 0.12)).abs() < 1e-9);
            }
        }

        #[test]
        fn when_the_model_predicts_then_the_in_form_asset_rises_and_the_out_of_form_asset_falls() {
            let season = price_season();
            let preds = PriceModel::fit(&examples(&season)).predict(&season);
            let by = |id: &str| {
                preds
                    .iter()
                    .find(|p| p.asset_id.as_str() == id)
                    .unwrap()
                    .change
            };
            assert!(by("d0") > 0.15);
            assert!(by("d5") < -0.15);
        }

        #[test]
        fn when_the_walk_forward_backtest_runs_then_the_model_beats_the_zero_baseline_with_perfect_direction(
        ) {
            let rep = backtest(&price_season());
            assert!(rep.examples > 0);
            assert!(rep.mae < rep.naive_mae);
            assert!((rep.direction - 1.0).abs() < 0.01);
        }
    }
}
