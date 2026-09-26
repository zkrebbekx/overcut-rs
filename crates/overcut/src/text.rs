//! Small text helpers shared by the CLI and the HTTP API.

/// Splits a comma-separated list into trimmed, non-empty tokens.
///
/// The tokens keep their case. The application layer upper-cases driver
/// codes when it uses them.
pub fn split_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

/// Splits an optional comma-separated list. `None` gives an empty list.
pub fn split_opt(raw: Option<&str>) -> Vec<String> {
    raw.map(split_list).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_a_list_with_spaces_and_empties_when_split_then_only_trimmed_tokens_remain() {
        assert_eq!(split_list(" VER, nor ,,HAM, "), vec!["VER", "nor", "HAM"]);
    }

    #[test]
    fn given_an_empty_string_when_split_then_the_list_is_empty() {
        assert!(split_list("").is_empty());
        assert!(split_list(" , ").is_empty());
    }

    #[test]
    fn given_no_value_when_split_then_the_list_is_empty() {
        assert!(split_opt(None).is_empty());
        assert_eq!(split_opt(Some("a,b")), vec!["a", "b"]);
    }
}
