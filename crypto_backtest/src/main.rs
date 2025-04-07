// main.rs
mod backtest;
mod fetch_data;
mod indicators;
mod models;
mod risk;
mod strategy;

use std::error::Error;
use crate::backtest::Backtester;
use crate::fetch_data::load_candles_from_csv;
use crate::strategy::{Strategy, StrategyConfig};

fn main() -> Result<(), Box<dyn Error>> {
    println!("Crypto Backtest Runner - Starting...");

    let csv_path = "data/BTC.csv";
    let mut candles = load_candles_from_csv(csv_path)?;
    println!("Loaded {} candles from CSV", candles.len());

    let original_count = candles.len();
    candles.retain(|c| c.volume > 0.0);

    if candles.len() < original_count {
        println!("Filtered out {} candles with zero volume", original_count - candles.len());
    }

    if candles.is_empty() {
        return Err("No candle data loaded".into());
    }

    println!("Data period: {} to {}", 
        &candles.first().unwrap().time,
        &candles.last().unwrap().time
    );

    let config = StrategyConfig {
        initial_balance: 10_000.0,
        leverage: 10.0,
        max_risk_per_trade: 0.01,
        pivot_lookback: 5,
        signal_lookback: 1,
        fib_limit1: 0.618,
        fib_limit2: 0.786,
    };

    let strategy = Strategy::new(config);
    let mut backtester = Backtester::new(10_000.0, strategy);

    println!("Running backtest...");
    let start_time = std::time::Instant::now();
    let results = backtester.run(&candles)?;
    let elapsed = start_time.elapsed();

    println!("\nBacktest completed in {:.2?}", elapsed);
    println!("Total trades: {}", results.metrics.total_trades);
    println!("Win rate: {:.2}%", results.metrics.win_rate * 100.0);
    println!("Profit factor: {:.2}", results.metrics.profit_factor);
    println!("Total profit: ${:.2}", results.metrics.total_profit);
    println!("Max drawdown: {:.2}%", results.metrics.max_drawdown * 100.0);
    println!("Sharpe ratio: {:.2}", results.metrics.sharpe_ratio);
    println!("Sortino ratio: {:.2}", results.metrics.sortino_ratio);

    Ok(())
}
