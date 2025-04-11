mod backtest;
mod fetch_data;
mod indicators;
mod models;
mod risk;
mod strategy;
mod optimizer;
mod stats; // <-- Add this to register the stats module

use std::error::Error;
use std::env;

use crate::backtest::Backtester;
use crate::fetch_data::load_candles_from_csv;
use crate::strategy::{Strategy, StrategyConfig};
use crate::optimizer::{optimize, OptimizationParams};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();
    let mode = args.get(1).map(|s| s.as_str()).unwrap_or("backtest");

    let csv_path = "data/BTC.csv";
    let mut candles = load_candles_from_csv(csv_path)?;
    candles.retain(|c| c.volume > 0.0);

    if candles.is_empty() {
        return Err("No candle data loaded".into());
    }

    match mode {
        "optimize" => run_optimizer(&candles),
        _ => run_single_backtest(&candles),
    }
}

fn run_single_backtest(candles: &[crate::models::Candle]) -> Result<(), Box<dyn Error>> {
    println!("Running single backtest...");

    let config = StrategyConfig {
        initial_balance: 10_000.0,
        leverage: 50.0,
        max_risk_per_trade: 0.01,
        pivot_lookback: 5,
        signal_lookback: 1,
        fib_limit1: 0.618,
        fib_limit2: 0.786,
        ..Default::default()
    };

    let strategy = Strategy::new(config.clone());
    let mut backtester = Backtester::new(config.initial_balance, strategy);

    let start_time = std::time::Instant::now();
    let results = backtester.run(candles)?;
    let elapsed = start_time.elapsed();

    println!("\nBacktest completed in {:.2?}", elapsed);
    println!("Total trades: {}", results.metrics.total_trades);
    println!("Win rate: {:.2}%", results.metrics.win_rate * 100.0);
    println!("Profit factor: {:.2}", results.metrics.profit_factor);
    println!("Total profit: ${:.2}", results.metrics.total_profit);
    println!("Max drawdown: {:.2}%", results.metrics.max_drawdown * 100.0);
    println!("Sharpe ratio: {:.2}", results.metrics.sharpe_ratio);
    println!("Sortino ratio: {:.2}", results.metrics.sortino_ratio);

    // Optional: print stats from StatsTracker
    println!("\nStats Summary:");
    println!("Avg PnL per trade: {:.2}", backtester.stats().average_pnl());
    println!("Equity Snapshots: {}", backtester.stats().equity_curve.len());

    Ok(())
}

fn run_optimizer(candles: &[crate::models::Candle]) -> Result<(), Box<dyn Error>> {
    println!("Running optimizer...");

    let params = OptimizationParams {
        fib_threshold: vec![5.0, 10.0, 15.0],
        fib_tp: vec![0.5, 0.618, 0.786],
        fib_sl: vec![0.2, 0.3],
        fib_initial: vec![0.236, 0.382],
    };

    let results = optimize(candles, params, 10_000.0);

    let mut writer = csv::Writer::from_path("optimization_results.csv")?;
    writer.write_record(&[
        "TP", "SL", "Init", "Threshold", "WinRate", "Profit", "Drawdown",
    ])?;

    for (config, metrics) in results {
        writer.write_record(&[
            config.fib_tp.to_string(),
            config.fib_sl.to_string(),
            config.fib_initial.to_string(),
            config.fib_threshold.to_string(),
            format!("{:.4}", metrics.win_rate),
            format!("{:.2}", metrics.total_profit),
            format!("{:.4}", metrics.max_drawdown),
        ])?;
    }

    writer.flush()?;
    println!("Results written to optimization_results.csv");

    Ok(())
}
