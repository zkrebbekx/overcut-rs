//! `overcut serve`: the JSON API and, optionally, the static web UI.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use clap::Args;
use tokio::net::TcpListener;

use super::{sync, GlobalOpts};
use crate::http::{self, AppState};

/// The `serve` flags.
#[derive(Debug, Clone, Args)]
pub struct ServeArgs {
    /// Listen address.
    #[arg(long, default_value = "127.0.0.1:8080")]
    pub addr: String,

    /// A built web app directory to serve as a single-page app. Without
    /// it, only the JSON API is served.
    #[arg(long, value_name = "DIR")]
    pub ui: Option<PathBuf>,
}

/// Runs the command.
pub async fn run(g: &GlobalOpts, args: &ServeArgs) -> anyhow::Result<()> {
    let ctx = g.context()?;
    let store = Arc::new(g.store());
    let state = AppState {
        ctx,
        sync: sync::use_case(Arc::clone(&store)),
        store,
        season: g.season,
    };
    let app = http::router(Arc::new(state), args.ui.as_deref());

    let listener = TcpListener::bind(&args.addr)
        .await
        .with_context(|| format!("listen on {}", args.addr))?;
    let addr = listener.local_addr().context("read listen address")?;
    println!("overcut serving on http://{addr}");
    axum::serve(listener, app).await.context("serve")
}
