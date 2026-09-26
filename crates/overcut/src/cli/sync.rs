//! `overcut sync`: download the season data.

use std::sync::Arc;

use clap::Args;
use overcut_application::usecases::{SyncSeason, SyncSources};
use overcut_domain::shared::RoundNumber;
use overcut_infrastructure::{
    FantasyFeedClient, JolpicaClient, JsonSeasonRepository, OfficialSiteClient, SystemClock,
};

use super::GlobalOpts;

/// The `sync` flags.
#[derive(Debug, Clone, Args)]
pub struct SyncArgs {
    /// Sync only when a session ended recently or the data is stale.
    #[arg(long)]
    pub when_due: bool,
}

/// Builds the sync use case over the public sources and the data file.
pub fn use_case(store: Arc<JsonSeasonRepository>) -> SyncSeason {
    let sources = SyncSources {
        race_data: Arc::new(JolpicaClient::default()),
        fantasy_feed: Arc::new(FantasyFeedClient::default()),
        starting_grid: Arc::new(OfficialSiteClient::default()),
        clock: Arc::new(SystemClock),
    };
    SyncSeason::new(sources, store)
}

/// Runs the command.
pub async fn run(g: &GlobalOpts, args: &SyncArgs) -> anyhow::Result<()> {
    let store = Arc::new(g.store());
    let path = store.path().display().to_string();
    let sync = use_case(Arc::clone(&store));

    if !args.when_due {
        println!("syncing season {}…", g.season);
    }
    let (view, season) = sync.execute(g.season, args.when_due).await?;
    if !view.synced {
        println!("skip: {}", view.reason);
        if let Some(next) = &view.next_session {
            println!("next: {next}");
        }
        return Ok(());
    }
    if args.when_due {
        println!("due: {}", view.reason);
        println!("syncing season {}…", g.season);
    }
    for n in &view.provisional_rounds {
        let seen = season
            .as_ref()
            .and_then(|s| RoundNumber::new(*n).ok().and_then(|r| s.round(r)))
            .and_then(|r| r.points_seen_at)
            .map(|t| t.to_rfc3339())
            .unwrap_or_default();
        println!("round {n} points are provisional (seen {seen})");
    }
    println!(
        "saved {path}: {} rounds ({} complete), {} assets",
        view.rounds, view.completed_rounds, view.assets
    );
    Ok(())
}
