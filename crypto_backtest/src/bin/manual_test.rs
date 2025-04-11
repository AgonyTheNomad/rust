// src/bin/manual_test.rs
use crypto_backtest::backtest::Backtester;
use crypto_backtest::fetch_data::load_candles_from_csv;
use crypto_backtest::strategy::{Strategy, StrategyConfig, AssetConfig};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    println!("Running manual test with specific parameters...");
    
    // Load data
    let csv_path = "data/BTC_small.csv";
    println!("Loading data from {}...", csv_path);
    let mut candles = load_candles_from_csv(csv_path)?;
    candles.retain(|c| c.volume > 0.0);
    
    println!("Testing with {} candles", candles.len());
    
    // Parameters from your position monitor
    let config = StrategyConfig {
        initial_balance: 10_000.0,
        leverage: 50.0,
        max_risk_per_trade: 0.02,
        pivot_lookback: 3,
        signal_lookback: 1,
        fib_threshold: 5.0,
        fib_initial: 0.5,
        fib_tp: 1.618,
        fib_sl: 0.5,
        fib_limit1: 0.618,
        fib_limit2: 1.272,
    };
    
    // Create an AssetConfig with all required fields
    let asset_config = AssetConfig {
        name: "BTC".to_string(),
        leverage: 50.0,
        spread: 0.0005,      // 0.05% spread
        avg_spread: 0.001,   // 0.1% average spread
    };
    
    let strategy = Strategy::new(config.clone(), asset_config);
    let mut backtester = Backtester::new(config.initial_balance, strategy);
    
    // Run the test
    let results = backtester.run(&candles)?;
    
    // Print the results
    println!("\nBacktest Results:");
    println!("Total trades: {}", results.metrics.total_trades);
    println!("Win rate: {:.2}%", results.metrics.win_rate * 100.0);
    println!("Profit factor: {:.2}", results.metrics.profit_factor);
    println!("Total profit: ${:.2}", results.metrics.total_profit);
    println!("Return: {:.2}%", results.metrics.total_profit / config.initial_balance * 100.0);
    println!("Max drawdown: {:.2}%", results.metrics.max_drawdown * 100.0);
    println!("Sharpe ratio: {:.2}", results.metrics.sharpe_ratio);
    println!("Sortino ratio: {:.2}", results.metrics.sortino_ratio);
    
    // Print trade details
    if !results.trades.is_empty() {
        println!("\nTrade details:");
        for (i, trade) in results.trades.iter().enumerate() {
            println!("Trade #{}: {} from {} to {}, Entry: ${:.2}, Exit: ${:.2}, PnL: ${:.2}",
                i + 1,
                trade.position_type,
                trade.entry_time,
                trade.exit_time,
                trade.entry_price,
                trade.exit_price,
                trade.pnl
            );
        }
    }
    
    Ok(())
}