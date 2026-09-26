//! The command line.
//!
//! Every command shares the global options: the data file, the season
//! year, and an optional scoring-rules override. Each command lives in its
//! own module and returns `anyhow::Result<()>`.

mod backtest;
mod hindsight;
mod optimize;
mod prices;
mod project;
mod review;
mod serve;
pub mod sync;

use std::ffi::OsStr;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context};
use clap::{Args, Parser, Subcommand};
use overcut_application::dto::ConditionsInput;
use overcut_application::AnalysisContext;
use overcut_domain::rules::ScoringRules;
use overcut_domain::season::{Season, SeasonRepository};
use overcut_infrastructure::{load_rules, JsonSeasonRepository};
use tabwriter::TabWriter;

use crate::text::split_opt;

/// The default season year.
pub const DEFAULT_SEASON: u16 = 2026;

/// The `overcut` command line.
#[derive(Debug, Parser)]
#[command(name = "overcut", version, about = "overcut — F1 Fantasy analysis toolkit", long_about = None)]
#[command(subcommand_required = true, arg_required_else_help = true)]
pub struct Cli {
    /// The options every command shares.
    #[command(flatten)]
    pub global: GlobalOpts,

    /// The command to run.
    #[command(subcommand)]
    pub command: Command,
}

/// The options every command shares.
#[derive(Debug, Clone, Args)]
pub struct GlobalOpts {
    /// Dataset file (default `~/.overcut/season<year>.json`).
    #[arg(long, global = true, value_name = "PATH")]
    pub data: Option<PathBuf>,

    /// Season year.
    #[arg(long, global = true, value_name = "YEAR", default_value_t = DEFAULT_SEASON)]
    pub season: u16,

    /// Scoring-rules JSON override.
    #[arg(long, global = true, value_name = "PATH")]
    pub rules: Option<PathBuf>,
}

/// The commands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Download season data.
    Sync(sync::SyncArgs),
    /// Project the next round.
    Project(project::ProjectArgs),
    /// Find the best team.
    Optimize(optimize::OptimizeArgs),
    /// Predict price changes.
    Prices,
    /// Measure model accuracy.
    Backtest(backtest::BacktestArgs),
    /// Best team for a past round.
    Hindsight(hindsight::HindsightArgs),
    /// Projection vs actual for a past round.
    Review(review::ReviewArgs),
    /// Web UI + JSON API.
    Serve(serve::ServeArgs),
}

impl Cli {
    /// Runs the selected command.
    pub async fn run(self) -> anyhow::Result<()> {
        let g = &self.global;
        match self.command {
            Command::Sync(args) => sync::run(g, &args).await,
            Command::Project(args) => project::run(g, &args),
            Command::Optimize(args) => optimize::run(g, &args),
            Command::Prices => prices::run(g),
            Command::Backtest(args) => backtest::run(g, &args),
            Command::Hindsight(args) => hindsight::run(g, &args),
            Command::Review(args) => review::run(g, &args),
            Command::Serve(args) => serve::run(g, &args).await,
        }
    }
}

/// Builds the default data path: `<home>/.overcut/season<year>.json`.
/// A missing home falls back to the current directory.
pub fn default_data_path(home: Option<&OsStr>, season: u16) -> PathBuf {
    let base = home.map_or_else(|| PathBuf::from("."), PathBuf::from);
    base.join(".overcut").join(format!("season{season}.json"))
}

impl GlobalOpts {
    /// The data file path: the `--data` value, or the default.
    pub fn data_path(&self) -> PathBuf {
        self.data
            .clone()
            .unwrap_or_else(|| default_data_path(std::env::var_os("HOME").as_deref(), self.season))
    }

    /// The season store at the data path.
    pub fn store(&self) -> JsonSeasonRepository {
        JsonSeasonRepository::new(self.data_path())
    }

    /// The scoring rules: the defaults, or the `--rules` file.
    pub fn load_rules(&self) -> anyhow::Result<ScoringRules> {
        match &self.rules {
            None => Ok(ScoringRules::default()),
            Some(path) => {
                load_rules(path).with_context(|| format!("load rules {}", path.display()))
            }
        }
    }

    /// Loads the season from the data file. A missing file is an error
    /// that points the user at `overcut sync`.
    pub fn load_season(&self) -> anyhow::Result<Season> {
        let store = self.store();
        store.load()?.ok_or_else(|| missing_dataset(store.path()))
    }

    /// Loads the season and the rules into an analysis context.
    pub fn context(&self) -> anyhow::Result<AnalysisContext> {
        let season = self.load_season()?;
        let rules = self.load_rules()?;
        Ok(AnalysisContext::new(season, rules))
    }
}

/// The error for a missing data file.
pub fn missing_dataset(path: &Path) -> anyhow::Error {
    anyhow!(
        "dataset {} not found (run `overcut sync` first)",
        path.display()
    )
}

/// The known-weekend flags that `project` and `optimize` share.
#[derive(Debug, Clone, Default, Args)]
pub struct ConditionArgs {
    /// Actual qualifying order, TLAs from P1 (e.g. RUS,HAM,VER,...).
    #[arg(long, value_name = "LIST")]
    pub quali: Option<String>,

    /// Actual starting grid after penalties, TLAs from P1.
    #[arg(long, value_name = "LIST")]
    pub grid: Option<String>,

    /// Drivers sent to the back of the grid, TLAs.
    #[arg(long, value_name = "LIST")]
    pub back: Option<String>,

    /// Practice order as a pace prior, TLAs from P1.
    #[arg(long, value_name = "LIST")]
    pub fp3: Option<String>,
}

impl ConditionArgs {
    /// Converts the flags to the application input.
    pub fn to_input(&self) -> ConditionsInput {
        ConditionsInput {
            quali: split_opt(self.quali.as_deref()),
            grid: split_opt(self.grid.as_deref()),
            back: split_opt(self.back.as_deref()),
            fp3: split_opt(self.fp3.as_deref()),
            ..ConditionsInput::default()
        }
    }
}

/// A tab-aligned table on stdout, with the same padding as the Go CLI.
pub fn table() -> TabWriter<io::Stdout> {
    TabWriter::new(io::stdout()).minwidth(0).padding(2)
}

/// Flushes a table to stdout.
pub fn flush(mut w: TabWriter<io::Stdout>) -> anyhow::Result<()> {
    w.flush().context("write table")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_a_home_when_the_default_path_is_built_then_it_is_under_dot_overcut() {
        let p = default_data_path(Some(OsStr::new("/home/zac")), 2026);
        assert_eq!(p, PathBuf::from("/home/zac/.overcut/season2026.json"));
    }

    #[test]
    fn given_no_home_when_the_default_path_is_built_then_it_is_relative_to_the_current_dir() {
        let p = default_data_path(None, 2025);
        assert_eq!(p, PathBuf::from("./.overcut/season2025.json"));
    }

    #[test]
    fn given_an_explicit_data_flag_when_the_path_is_resolved_then_the_flag_wins() {
        let g = GlobalOpts {
            data: Some(PathBuf::from("/tmp/x.json")),
            season: 2026,
            rules: None,
        };
        assert_eq!(g.data_path(), PathBuf::from("/tmp/x.json"));
    }

    #[test]
    fn given_condition_flags_when_converted_then_each_list_is_split() {
        let c = ConditionArgs {
            quali: Some("VER,NOR".into()),
            grid: None,
            back: Some(" HAM ".into()),
            fp3: None,
        };
        let input = c.to_input();
        assert_eq!(input.quali, vec!["VER", "NOR"]);
        assert!(input.grid.is_empty());
        assert_eq!(input.back, vec!["HAM"]);
    }

    #[test]
    fn given_the_cli_when_parsed_with_a_global_flag_after_the_command_then_it_is_accepted() {
        let cli = Cli::try_parse_from(["overcut", "project", "--season", "2025", "--sims", "10"])
            .unwrap();
        assert_eq!(cli.global.season, 2025);
        assert!(matches!(cli.command, Command::Project(ref a) if a.sims == 10));
    }
}
