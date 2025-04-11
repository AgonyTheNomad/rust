use crate::{Backtester, Strategy, StrategyConfig};
use crate::models::Candle;

use crate::backtest::BacktestMetrics;

#[derive(Debug)]
pub struct OptimizationParams {
    pub fib_threshold: Vec<f64>,
    pub fib_tp: Vec<f64>,
    pub fib_sl: Vec<f64>,
    pub fib_initial: Vec<f64>,
}

pub fn optimize(
    candles: &[Candle],
    params: OptimizationParams,
    initial_balance: f64,
) -> Vec<(StrategyConfig, BacktestMetrics)> {
    let mut results = Vec::new();

    for &threshold in &params.fib_threshold {
        for &tp in &params.fib_tp {
            for &sl in &params.fib_sl {
                for &initial in &params.fib_initial {
                    let config = StrategyConfig {
                        fib_threshold: threshold,
                        fib_tp: tp,
                        fib_sl: sl,
                        fib_initial: initial,
                        ..Default::default()
                    };

                    let strategy = Strategy::new(config.clone());
                    let mut backtester = Backtester::new(initial_balance, strategy);

                    if let Ok(result) = backtester.run(candles) {
                        results.push((config, result.metrics));
                    }
                }
            }
        }
    }

    results
}