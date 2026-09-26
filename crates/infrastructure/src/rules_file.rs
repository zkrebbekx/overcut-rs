//! The scoring-rules file loader.
//!
//! A rules file holds any subset of the fields of `RulesDto`. The loader
//! overlays the file onto the default rules. A field that the file does not
//! set keeps its default value.

use std::io;
use std::path::{Path, PathBuf};

use overcut_application::dto::RulesDto;
use overcut_domain::rules::ScoringRules;
use thiserror::Error;

/// A failure to load a rules file.
#[derive(Debug, Error)]
pub enum RulesFileError {
    /// The file could not be read.
    #[error("cannot read the rules file {}: {source}", path.display())]
    Read {
        /// The file path.
        path: PathBuf,
        /// The I/O error.
        #[source]
        source: io::Error,
    },
    /// The file is not valid rules JSON.
    #[error("cannot parse the rules file {}: {source}", path.display())]
    Parse {
        /// The file path.
        path: PathBuf,
        /// The JSON error.
        #[source]
        source: serde_json::Error,
    },
}

/// Loads the scoring rules from a JSON file. The file overrides the default
/// rules field by field.
pub fn load_rules(path: &Path) -> Result<ScoringRules, RulesFileError> {
    let text = std::fs::read_to_string(path).map_err(|source| RulesFileError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let dto: RulesDto = serde_json::from_str(&text).map_err(|source| RulesFileError::Parse {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(dto.apply_to(ScoringRules::default()))
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    #[test]
    fn given_a_partial_rules_file_when_loaded_then_only_the_listed_fields_change() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        write!(file, r#"{{"race_dnf": -15, "budget": 110.5, "constructor_dsq": {{"quali": -1, "sprint": -2, "race": -3}}}}"#)
            .unwrap();

        let rules = load_rules(file.path()).unwrap();

        let defaults = ScoringRules::default();
        assert_eq!(rules.race_dnf, -15);
        assert_eq!(rules.budget, 110.5);
        assert_eq!(rules.constructor_dsq.race, -3);
        assert_eq!(rules.quali_no_time, defaults.quali_no_time);
        assert_eq!(rules.race_points, defaults.race_points);
    }

    #[test]
    fn given_a_missing_file_when_loaded_then_a_read_error_names_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("absent.json");

        let err = load_rules(&path).unwrap_err();

        assert!(matches!(err, RulesFileError::Read { .. }));
        assert!(err.to_string().contains("absent.json"));
    }

    #[test]
    fn given_malformed_json_when_loaded_then_a_parse_error_is_returned() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        write!(file, "{{ not json").unwrap();

        let err = load_rules(file.path()).unwrap_err();

        assert!(matches!(err, RulesFileError::Parse { .. }));
    }
}
