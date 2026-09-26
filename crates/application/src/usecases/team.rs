//! Resolving a user's team tokens to asset identifiers.

use overcut_domain::season::Season;
use overcut_domain::shared::AssetId;

use crate::AppError;

/// Maps user tokens (driver codes or constructor-name prefixes) to asset
/// identifiers. Empty tokens are skipped.
pub fn resolve_team(season: &Season, tokens: &[String]) -> Result<Vec<AssetId>, AppError> {
    tokens
        .iter()
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .map(|t| {
            season
                .find_asset(t)
                .map(|a| a.id.clone())
                .map_err(AppError::from)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use overcut_domain::season::Asset;
    use overcut_domain::shared::{AssetKind, TeamId, Tla};

    use super::*;

    fn season() -> Season {
        let asset = |id: &str, kind, name: &str, tla: Option<&str>| Asset {
            id: AssetId::new(id).unwrap(),
            kind,
            name: name.into(),
            tla: tla.map(|t| Tla::parse(t).unwrap()),
            team_id: TeamId::new("t").unwrap(),
            team_name: "T".into(),
            history: vec![],
        };
        Season::new(
            2026,
            Utc::now(),
            vec![],
            vec![
                asset("1", AssetKind::Driver, "Max Verstappen", Some("VER")),
                asset("2", AssetKind::Constructor, "McLaren", None),
            ],
        )
        .unwrap()
    }

    #[test]
    fn given_a_code_and_a_name_prefix_when_resolved_then_both_map_to_identifiers() {
        let ids = resolve_team(&season(), &["ver".into(), " mcl".into(), String::new()]).unwrap();
        assert_eq!(
            ids,
            vec![AssetId::new("1").unwrap(), AssetId::new("2").unwrap()]
        );
    }

    #[test]
    fn given_an_unknown_token_when_resolved_then_the_error_names_it() {
        let err = resolve_team(&season(), &["XXX".into()]).unwrap_err();
        assert!(err.to_string().contains("XXX"));
    }
}
