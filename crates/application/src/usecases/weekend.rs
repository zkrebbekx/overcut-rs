//! Shared helpers: the known weekend state and the projection view.

use std::collections::BTreeSet;

use overcut_domain::projection::{SimulationResult, WeekendConditions};
use overcut_domain::season::{Round, Season};
use overcut_domain::shared::Tla;

use crate::dto::{AssetProjection, ConditionsInput, ProjectionView};
use crate::AppError;

/// Which parts of the weekend state came from the data.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Known {
    pub quali: bool,
    pub grid: bool,
    pub sprint: bool,
}

fn codes(list: &[String]) -> Vec<String> {
    list.iter()
        .map(|s| s.trim().to_ascii_uppercase())
        .filter(|s| !s.is_empty())
        .collect()
}

fn labels(order: &[Tla]) -> Vec<String> {
    order.iter().map(ToString::to_string).collect()
}

/// Fills an empty qualifying order, grid, and sprint result from the
/// official data: the published grid before the race, the race
/// classification after it, the sprint classification once the sprint has
/// run. The grid carries every penalty, so it takes precedence over
/// back-of-grid hints.
pub(crate) fn with_known_weekend(
    target: &Round,
    mut cond: ConditionsInput,
) -> (ConditionsInput, Known) {
    let mut known = Known::default();
    if cond.quali.is_empty() {
        if let Some(order) = target.quali_order() {
            cond.quali = labels(&order);
            known.quali = true;
        }
    }
    if cond.grid.is_empty() {
        if let Some(order) = target.grid_order() {
            cond.grid = labels(&order);
            known.grid = true;
        }
    }
    if target.has_sprint && cond.sprint.is_empty() {
        if let Some(sprint) = target.sprint_order() {
            cond.sprint = labels(&sprint.finish);
            cond.sprint_grid = labels(&sprint.grid);
            cond.sprint_dnf = sprint.dnf.iter().map(ToString::to_string).collect();
            known.sprint = true;
        }
    }
    (cond, known)
}

/// Converts the client's orders to domain conditions. A code that fails to
/// parse is an input error.
pub(crate) fn to_domain(input: &ConditionsInput) -> Result<WeekendConditions, AppError> {
    let parse = |list: &[String]| -> Result<Vec<Tla>, AppError> {
        codes(list)
            .iter()
            .map(|c| Tla::parse(c).map_err(AppError::from))
            .collect()
    };
    let mut cond = WeekendConditions {
        quali: WeekendConditions::positions(&parse(&input.quali)?),
        grid: WeekendConditions::positions(&parse(&input.grid)?),
        back_of_grid: parse(&input.back)?.into_iter().collect(),
        practice: WeekendConditions::positions(&parse(&input.fp3)?),
        ..Default::default()
    };
    if !input.sprint.is_empty() {
        let dnf: BTreeSet<Tla> = parse(&input.sprint_dnf)?.into_iter().collect();
        cond = cond.with_sprint(&parse(&input.sprint)?, &parse(&input.sprint_grid)?, dnf);
    }
    Ok(cond)
}

/// Builds the projection view of one round from its simulation.
pub(crate) fn projection_view(
    season: &Season,
    target: &Round,
    sim: &SimulationResult,
    cond: ConditionsInput,
    known: Known,
) -> ProjectionView {
    let gameday = target.number.gameday();
    let mut assets: Vec<AssetProjection> = season
        .assets()
        .iter()
        .filter_map(|a| {
            let p = sim.by_id(&a.id)?;
            let latest = a.latest();
            let actual = if target.has_results {
                a.at(gameday)
            } else {
                None
            };
            Some(AssetProjection {
                id: a.id.to_string(),
                name: a.name.clone(),
                kind: a.kind.as_str().to_string(),
                tla: a.tla.as_ref().map(ToString::to_string).unwrap_or_default(),
                team_id: a.team_id.to_string(),
                team_name: a.team_label().to_string(),
                price: latest.map_or(0.0, |h| h.price),
                ownership: latest.map_or(0.0, |h| h.ownership),
                mean: p.dist.mean,
                sd: p.dist.sd,
                p10: p.dist.p10,
                p50: p.dist.p50,
                p90: p.dist.p90,
                last_points: a.previous_points(gameday),
                avg_points: a.season_mean_before(gameday),
                actual_points: actual.map_or(0.0, |h| h.points),
                has_actual: actual.is_some(),
            })
        })
        .collect();
    assets.sort_by(|a, b| b.mean.total_cmp(&a.mean));
    ProjectionView {
        round: target.number.get(),
        name: target.name.clone(),
        has_sprint: target.has_sprint,
        sims: sim.sims,
        conditions: cond,
        quali_from_data: known.quali,
        grid_from_data: known.grid,
        sprint_from_data: known.sprint,
        assets,
    }
}

/// Parses a round number from the API's zero-means-default convention.
pub(crate) fn round_number(n: u32) -> Option<overcut_domain::shared::RoundNumber> {
    overcut_domain::shared::RoundNumber::new(n).ok()
}
