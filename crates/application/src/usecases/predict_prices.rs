//! Predict the next price change per asset.

use overcut_domain::pricing::{self, PriceModel};

use crate::dto::{PriceBacktestView, PricePredictionView, PricesView};

use super::AnalysisContext;

/// Fits the price model on the season's real moves and predicts the next
/// change for every selectable asset.
pub struct PredictPrices<'a> {
    ctx: &'a AnalysisContext,
}

impl<'a> PredictPrices<'a> {
    /// Binds the use case to a context.
    pub fn new(ctx: &'a AnalysisContext) -> Self {
        Self { ctx }
    }

    /// Runs the prediction and the walk-forward accuracy report.
    pub fn execute(&self) -> PricesView {
        let season = self.ctx.season();
        let model = PriceModel::fit(&pricing::examples(&season));
        let rep = pricing::backtest(&season);
        PricesView {
            report: PriceBacktestView {
                examples: rep.examples,
                mae: rep.mae,
                naive_mae: rep.naive_mae,
                direction: rep.direction,
                moves: rep.moves,
            },
            predictions: model
                .predict(&season)
                .into_iter()
                .map(|p| PricePredictionView {
                    asset_id: p.asset_id.to_string(),
                    name: p.name,
                    kind: p.kind.as_str().to_string(),
                    price: p.price,
                    change: p.change,
                })
                .collect(),
        }
    }
}
