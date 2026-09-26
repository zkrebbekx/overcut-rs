//! The official Formula 1 website gateway.
//!
//! The results pages of the official site publish the starting grid a few
//! hours after qualifying, with every penalty applied. That page is the
//! earliest public record of where each car starts.
//!
//! The adapter reads two pages:
//!
//! 1. The season index at `/en/results/{season}/races`. It lists every race
//!    with a numeric id and a slug, such as `1293` and `italy`.
//! 2. The grid page at `/en/results/{season}/races/{id}/{slug}/starting-grid`.
//!    Its first table holds one row per car.
//!
//! The parsing functions are public and pure so a test can run them on a
//! saved page.

use std::sync::LazyLock;
use std::time::Duration;

use async_trait::async_trait;
use overcut_application::ports::{GatewayError, GridSlot, StartingGridGateway};
use overcut_domain::shared::Tla;
use regex::Regex;
use reqwest::StatusCode;

/// The site root.
const DEFAULT_BASE_URL: &str = "https://www.formula1.com";

/// The gateway name in every error.
const GATEWAY: &str = "f1site";

/// The User-Agent header the client sends.
use crate::USER_AGENT;

static RACE_LINK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"/en/results/(\d{4})/races/(\d+)/([a-z0-9-]+)/").expect("valid regex")
});
static TABLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<table.*?</table>").expect("valid regex"));
static ROW: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<tr.*?</tr>").expect("valid regex"));
static CELL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<t[dh][^>]*>(.*?)</t[dh]>").expect("valid regex"));
static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]+>").expect("valid regex"));
static SPACE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").expect("valid regex"));

/// One entry of the season results index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaceLink {
    /// The site's numeric race id.
    pub id: u32,
    /// The site's country or circuit slug, such as `italy` or
    /// `great-britain`.
    pub slug: String,
}

/// One car on the starting grid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GridRow {
    /// The grid position from P1.
    pub position: u32,
    /// The car number.
    pub number: u32,
    /// The three-letter driver code.
    pub code: String,
    /// The team name.
    pub team: String,
}

/// Extracts the race ids and slugs of one season from the index page. Each
/// race appears once, in page order.
pub fn parse_index(page: &str, season: u16) -> Vec<RaceLink> {
    let season = season.to_string();
    let mut out: Vec<RaceLink> = Vec::new();
    for captures in RACE_LINK.captures_iter(page) {
        if captures[1] != season {
            continue;
        }
        let Ok(id) = captures[2].parse::<u32>() else {
            continue;
        };
        if out.iter().any(|r| r.id == id) {
            continue;
        }
        out.push(RaceLink {
            id,
            slug: captures[3].to_string(),
        });
    }
    out
}

/// Finds the index entry for a race name such as "Italian Grand Prix". The
/// match is case-insensitive. Returns `None` when no slug matches.
pub fn match_race<'a>(index: &'a [RaceLink], race_name: &str) -> Option<&'a RaceLink> {
    let name = race_name.to_lowercase();
    index.iter().find(|r| name.contains(slug_key(&r.slug)))
}

/// Maps a site slug to a word that appears in the race name of the
/// classification source. A slug without an alias matches on its first
/// word.
fn slug_key(slug: &str) -> &str {
    match slug {
        "australia" => "australian",
        "china" => "chinese",
        "japan" => "japanese",
        "canada" => "canadian",
        "barcelona-catalunya" => "barcelona",
        "austria" => "austrian",
        "great-britain" => "british",
        "belgium" => "belgian",
        "hungary" => "hungarian",
        "netherlands" => "dutch",
        "italy" => "italian",
        "spain" => "spanish",
        "brazil" => "brazil",
        "emilia-romagna" => "emilia",
        "saudi-arabia" => "saudi",
        "united-states" => "united states",
        "las-vegas" => "las vegas",
        "abu-dhabi" => "abu dhabi",
        _ => slug.split('-').next().unwrap_or(slug),
    }
}

/// Extracts the grid rows from a starting-grid page. Returns an empty list
/// when the page has no grid table yet.
///
/// The parser reads the first `<table>`. Each `<tr>` yields the text of its
/// `<td>` and `<th>` cells with tags removed, entities decoded, and
/// whitespace collapsed. A row is a car when it has at least four cells, the
/// first cell is a number, and the last word of the third cell is a
/// three-letter upper-case code.
pub fn parse_grid(page: &str) -> Vec<GridRow> {
    let Some(table) = TABLE.find(page) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for row in ROW.find_iter(table.as_str()) {
        let cells: Vec<String> = CELL
            .captures_iter(row.as_str())
            .map(|c| {
                let text = unescape_html(&TAG.replace_all(&c[1], " "));
                SPACE.replace_all(&text, " ").trim().to_string()
            })
            .collect();
        if cells.len() < 4 {
            continue;
        }
        // A header row has a label in the position cell.
        let Ok(position) = cells[0].parse::<u32>() else {
            continue;
        };
        let number = cells[1].parse::<u32>().unwrap_or(0);
        let Some(code) = cells[2].split_whitespace().last() else {
            continue;
        };
        if code.len() != 3 || code.to_uppercase() != code {
            continue;
        }
        out.push(GridRow {
            position,
            number,
            code: code.to_string(),
            team: cells[3].clone(),
        });
    }
    out
}

/// Decodes the HTML entities that appear on the results pages: the five
/// named XML entities, `&nbsp;`, and numeric references. An unknown entity
/// stays as written.
fn unescape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        let decoded = after
            .find(';')
            .filter(|end| *end <= 10)
            .and_then(|end| decode_entity(&after[1..end]).map(|c| (c, end)));
        if let Some((c, end)) = decoded {
            out.push(c);
            rest = &after[end + 1..];
        } else {
            out.push('&');
            rest = &after[1..];
        }
    }
    out.push_str(rest);
    out
}

fn decode_entity(entity: &str) -> Option<char> {
    match entity {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some(' '),
        _ => {
            let digits = entity.strip_prefix('#')?;
            let code = match digits.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => digits.parse().ok()?,
            };
            char::from_u32(code)
        }
    }
}

/// Reads the official starting grid from the Formula 1 website.
#[derive(Debug, Clone)]
pub struct OfficialSiteClient {
    base_url: String,
    http: reqwest::Client,
}

impl OfficialSiteClient {
    /// Builds a client against `base_url`. The client removes a trailing
    /// slash. Every request carries the overcut User-Agent and times out
    /// after 30 seconds.
    pub fn new(base_url: impl Into<String>) -> Self {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_default();
        Self { base_url, http }
    }

    /// The site root.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    async fn get(&self, url: &str) -> Result<String, GatewayError> {
        let response = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|e| GatewayError::new(GATEWAY, format!("get {url}: {e}")))?;
        let status = response.status();
        if status != StatusCode::OK {
            return Err(GatewayError::new(
                GATEWAY,
                format!("get {url}: status {}", status.as_u16()),
            ));
        }
        response
            .text()
            .await
            .map_err(|e| GatewayError::new(GATEWAY, format!("read {url}: {e}")))
    }
}

impl Default for OfficialSiteClient {
    fn default() -> Self {
        Self::new(DEFAULT_BASE_URL)
    }
}

#[async_trait]
impl StartingGridGateway for OfficialSiteClient {
    async fn starting_grid(
        &self,
        season: u16,
        race_name: &str,
    ) -> Result<Option<Vec<GridSlot>>, GatewayError> {
        let index_page = self
            .get(&format!("{}/en/results/{season}/races", self.base_url))
            .await?;
        let index = parse_index(&index_page, season);
        let Some(race) = match_race(&index, race_name) else {
            tracing::debug!(
                race_name,
                "the official results index has no entry for the race"
            );
            return Ok(None);
        };
        let grid_url = format!(
            "{}/en/results/{season}/races/{}/{}/starting-grid",
            self.base_url, race.id, race.slug
        );
        let rows = parse_grid(&self.get(&grid_url).await?);
        if rows.is_empty() {
            return Ok(None);
        }
        let slots = rows
            .iter()
            .filter_map(|row| {
                Tla::parse(&row.code).ok().map(|tla| GridSlot {
                    tla,
                    position: row.position,
                })
            })
            .collect();
        Ok(Some(slots))
    }
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    const INDEX_PAGE: &str = r#"
<a href="/en/results/2026/races/1292/netherlands/race-result">Netherlands</a>
<a href="/en/results/2026/races/1293/italy/race-result">Italy</a>
<a href="/en/results/2026/races/1293/italy/qualifying">Italy</a>
<a href="/en/results/2025/races/1276/abu-dhabi/race-result">old</a>
<a href="/en/results/2026/races/1308/bahrain/race-result">Bahrain</a>
"#;

    const GRID_PAGE: &str = r"
<table>
<tr><th>Pos.</th><th>No.</th><th>Driver</th><th>Team</th><th>Time</th></tr>
<tr><td>1</td><td>10</td><td><span>Pierre</span> <span>Gasly</span> <span>GAS</span></td><td>Alpine</td><td>1:21.786</td></tr>
<tr><td>2</td><td>63</td><td>George Russell RUS</td><td>Mercedes</td><td>1:21.846</td></tr>
<tr><td>20</td><td>12</td><td>Kimi Antonelli ANT</td><td>Mercedes</td><td>1:22.093</td></tr>
</table>
";

    fn link(id: u32, slug: &str) -> RaceLink {
        RaceLink {
            id,
            slug: slug.to_string(),
        }
    }

    #[test]
    fn given_a_season_index_page_when_parsed_for_2026_then_each_race_appears_once_with_id_and_slug()
    {
        let races = parse_index(INDEX_PAGE, 2026);
        assert_eq!(
            races,
            vec![
                link(1292, "netherlands"),
                link(1293, "italy"),
                link(1308, "bahrain")
            ]
        );
    }

    #[test]
    fn given_the_index_when_a_race_name_is_matched_then_the_aliased_slug_resolves() {
        let index = parse_index(INDEX_PAGE, 2026);
        let race = match_race(&index, "Italian Grand Prix").unwrap();
        assert_eq!(race.id, 1293);
        assert_eq!(
            match_race(&index, "Dutch Grand Prix").map(|r| r.id),
            Some(1292)
        );
    }

    #[test]
    fn given_the_index_when_an_absent_race_is_matched_then_there_is_no_match() {
        let index = parse_index(INDEX_PAGE, 2026);
        assert_eq!(match_race(&index, "Monaco Grand Prix"), None);
    }

    #[test]
    fn given_an_unaliased_slug_when_matched_then_its_first_word_is_used() {
        let index = vec![
            link(1, "miami"),
            link(2, "great-britain"),
            link(3, "united-states"),
        ];
        assert_eq!(
            match_race(&index, "Miami Grand Prix").map(|r| r.id),
            Some(1)
        );
        assert_eq!(
            match_race(&index, "British Grand Prix").map(|r| r.id),
            Some(2)
        );
        assert_eq!(
            match_race(&index, "United States Grand Prix").map(|r| r.id),
            Some(3)
        );
    }

    #[test]
    fn given_a_starting_grid_page_when_parsed_then_every_car_row_yields_position_number_code_and_team(
    ) {
        let rows = parse_grid(GRID_PAGE);
        assert_eq!(rows.len(), 3);
        assert_eq!(
            rows[0],
            GridRow {
                position: 1,
                number: 10,
                code: "GAS".into(),
                team: "Alpine".into()
            }
        );
        assert_eq!(
            rows[1],
            GridRow {
                position: 2,
                number: 63,
                code: "RUS".into(),
                team: "Mercedes".into()
            }
        );
        assert_eq!(
            rows[2],
            GridRow {
                position: 20,
                number: 12,
                code: "ANT".into(),
                team: "Mercedes".into()
            }
        );
    }

    #[test]
    fn given_a_page_without_a_table_when_parsed_then_the_result_is_empty() {
        assert!(parse_grid("<html><body>Not yet</body></html>").is_empty());
    }

    #[test]
    fn given_a_cell_with_entities_when_parsed_then_the_text_is_decoded() {
        let page = "<table><tr><td>1</td><td>4</td><td>Lando&nbsp;Norris&#32;NOR</td><td>McLaren &amp; Co &#x26; Ltd</td></tr></table>";
        let rows = parse_grid(page);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].code, "NOR");
        assert_eq!(rows[0].team, "McLaren & Co & Ltd");
    }

    #[test]
    fn given_a_row_with_a_lowercase_last_word_when_parsed_then_the_row_is_skipped() {
        let page =
            "<table><tr><td>1</td><td>4</td><td>Lando Norris nor</td><td>McLaren</td></tr></table>";
        assert!(parse_grid(page).is_empty());
    }

    #[test]
    fn given_an_unknown_entity_when_unescaped_then_it_stays_as_written() {
        assert_eq!(
            unescape_html("a &bogus; b &amp c &lt;"),
            "a &bogus; b &amp c <"
        );
    }

    #[tokio::test]
    async fn given_the_site_when_a_grid_is_fetched_end_to_end_then_the_slots_are_returned() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/en/results/2026/races"))
            .and(header("user-agent", USER_AGENT))
            .respond_with(ResponseTemplate::new(200).set_body_string(INDEX_PAGE))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/en/results/2026/races/1293/italy/starting-grid"))
            .and(header("user-agent", USER_AGENT))
            .respond_with(ResponseTemplate::new(200).set_body_string(GRID_PAGE))
            .expect(1)
            .mount(&server)
            .await;
        let client = OfficialSiteClient::new(server.uri());

        let grid = client
            .starting_grid(2026, "Italian Grand Prix")
            .await
            .unwrap()
            .unwrap();

        assert_eq!(
            grid,
            vec![
                GridSlot {
                    tla: Tla::parse("GAS").unwrap(),
                    position: 1
                },
                GridSlot {
                    tla: Tla::parse("RUS").unwrap(),
                    position: 2
                },
                GridSlot {
                    tla: Tla::parse("ANT").unwrap(),
                    position: 20
                },
            ]
        );
    }

    #[tokio::test]
    async fn given_the_site_when_the_race_is_not_in_the_index_then_the_grid_is_none() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/en/results/2026/races"))
            .respond_with(ResponseTemplate::new(200).set_body_string(INDEX_PAGE))
            .mount(&server)
            .await;
        let client = OfficialSiteClient::new(server.uri());

        assert_eq!(
            client
                .starting_grid(2026, "Monaco Grand Prix")
                .await
                .unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn given_the_site_when_the_grid_page_has_no_table_yet_then_the_grid_is_none() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/en/results/2026/races"))
            .respond_with(ResponseTemplate::new(200).set_body_string(INDEX_PAGE))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/en/results/2026/races/1293/italy/starting-grid"))
            .respond_with(ResponseTemplate::new(200).set_body_string("<html>Not yet</html>"))
            .mount(&server)
            .await;
        let client = OfficialSiteClient::new(server.uri());

        assert_eq!(
            client
                .starting_grid(2026, "Italian Grand Prix")
                .await
                .unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn given_the_site_when_the_index_returns_an_error_then_a_gateway_error_is_returned() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/en/results/2026/races"))
            .respond_with(ResponseTemplate::new(429))
            .mount(&server)
            .await;
        let client = OfficialSiteClient::new(server.uri());

        let err = client
            .starting_grid(2026, "Italian Grand Prix")
            .await
            .unwrap_err();

        assert_eq!(err.gateway, "f1site");
        assert!(err.message.contains("status 429"), "{}", err.message);
    }
}
