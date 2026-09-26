//! `overcut prices`: predict the next price change per asset.

use std::io::Write;

use overcut_application::usecases::PredictPrices;

use super::{flush, table, GlobalOpts};

/// Runs the command.
pub fn run(g: &GlobalOpts) -> anyhow::Result<()> {
    let ctx = g.context()?;
    let view = PredictPrices::new(&ctx).execute();
    let r = &view.report;
    println!(
        "Predicted next price changes (model MAE ${:.3}M vs ${:.3}M naive, {:.0}% direction hit rate on {} moves)\n",
        r.mae,
        r.naive_mae,
        r.direction * 100.0,
        r.moves
    );
    let mut w = table();
    writeln!(w, "ASSET\tKIND\tPRICE\tPREDICTED Δ")?;
    for p in &view.predictions {
        writeln!(
            w,
            "{}\t{}\t{:.1}\t{:+.2}",
            p.name, p.kind, p.price, p.change
        )?;
    }
    flush(w)
}
