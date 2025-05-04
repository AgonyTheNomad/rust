// src/bin/debug_optimizer.rs
use std::error::Error;
use std::path::Path;
use std::collections::HashMap;

use crypto_backtest::fetch_data::load_candles_from_csv;
use crypto_backtest::strategy::{Strategy, StrategyConfig, AssetConfig};
use crypto_backtest::backtest::Backtester;
use crypto_backtest::models::{default_strategy_config, default_asset_config};

// Add this custom function to get position details from the backtest results
fn get_position_details(results: &crypto_backtest::backtest::BacktestResults) -> Vec<String> {
    let mut details = Vec::new();
    let trades = &results.trades;
    
    if trades.is_empty() {
        return vec!["No trades executed.".to_string()];
    }
    
    // We need to extract position information from strategy interactions
    // Since we don't have direct access to position details through trades,
    // we'll simulate what we can based on the trade data
    
    for (i, trade) in trades.iter().enumerate() {
        let mut detail = format!("Trade #{}: {} position", i+1, trade.position_type);
        detail.push_str(&format!("\n  Entry Time: {}", trade.entry_time));
        detail.push_str(&format!("\n  Exit Time: {}", trade.exit_time));
        detail.push_str(&format!("\n  Entry Price: ${:.2}", trade.entry_price));
        detail.push_str(&format!("\n  Exit Price: ${:.2}", trade.exit_price));
        
        // Calculate implied stop loss and take profit based on position type
        let implied_tp = if trade.position_type == "Long" {
            trade.entry_price * 1.02  // Assume 2% TP for demonstration
        } else {
            trade.entry_price * 0.98
        };
        
        let implied_sl = if trade.position_type == "Long" {
            trade.entry_price * 0.99  // Assume 1% SL for demonstration
        } else {
            trade.entry_price * 1.01
        };
        
        detail.push_str(&format!("\n  Implied Take Profit: ${:.2}", implied_tp));
        detail.push_str(&format!("\n  Implied Stop Loss: ${:.2}", implied_sl));
        detail.push_str(&format!("\n  Size: {:.6}", trade.size));
        detail.push_str(&format!("\n  P&L: ${:.2}", trade.pnl));
        detail.push_str(&format!("\n  Risk %: {:.2}%", trade.risk_percent * 100.0));
        
        details.push(detail);
    }
    
    details
}

fn main() -> Result<(), Box<dyn Error>> {
    // Load candle data
    let csv_path = "data/BTC.csv";
    println!("Loading data from {}...", csv_path);
    let mut candles = load_candles_from_csv(csv_path)?;
    
    // Filter invalid candles
    println!("Loaded {} raw candles", candles.len());
    candles.retain(|c| c.volume > 0.0);
    println!("Filtered to {} valid candles", candles.len());
    
    if candles.is_empty() {
        return Err("No valid candle data loaded".into());
    }
    
    // Print some sample candles to verify data format
    println!("\nSample candle data (first 3 records):");
    for (i, candle) in candles.iter().take(3).enumerate() {
        println!("Candle #{}: Time={}, Open={:.2}, High={:.2}, Low={:.2}, Close={:.2}, Volume={:.2}",
            i+1, candle.time, candle.open, candle.high, candle.low, candle.close, candle.volume);
    }
    
    // Define several fixed parameter sets to test
    let test_configs = vec![
        // Test config 1: Conservative
        {
            let mut config = default_strategy_config();
            config.name = "Conservative".to_string();
            config.leverage = 20.0;
            config.max_risk_per_trade = 0.01;
            config.pivot_lookback = 5;
            config.signal_lookback = 1;
            config.fib_threshold = 10.0;
            config.fib_initial = 0.382;
            config.fib_tp = 1.0;
            config.fib_sl = 0.5;
            config.fib_limit1 = 0.5;
            config.fib_limit2 = 1.0;
            config
        },
        // Test config 2: Aggressive
        {
            let mut config = default_strategy_config();
            config.name = "Aggressive".to_string();
            config.leverage = 50.0;
            config.max_risk_per_trade = 0.1;
            config.pivot_lookback = 3;
            config.signal_lookback = 1;
            config.fib_threshold = 5.0;
            config.fib_initial = 0.5;
            config.fib_tp = 1.618;
            config.fib_sl = 0.382;
            config.fib_limit1 = 0.618;
            config.fib_limit2 = 1.0;
            config
        },
        // Test config 3: Another variation
        {
            let mut config = default_strategy_config();
            config.name = "Balanced".to_string();
            config.leverage = 30.0;
            config.max_risk_per_trade = 0.1;
            config.pivot_lookback = 8;
            config.signal_lookback = 2;
            config.fib_threshold = 15.0;
            config.fib_initial = 0.618;
            config.fib_tp = 2.0;
            config.fib_sl = 0.618;
            config.fib_limit1 = 0.786;
            config.fib_limit2 = 1.272;
            config
        },
    ];
    
    // Run each config and analyze results
    println!("\nTesting {} different parameter configurations", test_configs.len());
    
    let mut all_trades_info = HashMap::new();
    
    for (i, config) in test_configs.iter().enumerate() {
        println!("\n--------------------------------------------------");
        println!("TESTING CONFIGURATION #{}", i + 1);
        println!("--------------------------------------------------");
        println!("Parameters:");
        println!("  Lookback: {}", config.pivot_lookback);
        println!("  Signal Lookback: {}", config.signal_lookback);
        println!("  Threshold: {:.2}", config.fib_threshold);
        println!("  Initial Level: {:.3}", config.fib_initial);
        println!("  Take Profit: {:.3}", config.fib_tp);
        println!("  Stop Loss: {:.3}", config.fib_sl);
        println!("  Limit1: {:.3}", config.fib_limit1);
        println!("  Limit2: {:.3}", config.fib_limit2);
        println!("  Leverage: {:.1}x", config.leverage);
        println!("  Risk per Trade: {:.2}%", config.max_risk_per_trade * 100.0);
        
        // Create asset config for this strategy
        let asset_config = default_asset_config("BTC");
        let strategy = Strategy::new(config.clone(), asset_config);
        let mut backtester = Backtester::new(config.initial_balance, strategy);
        
        match backtester.run(&candles) {
            Ok(results) => {
                println!("\nBacktest Results:");
                println!("  Execution time: {:.2?}", results.duration);
                println!("  Total trades: {}", results.metrics.total_trades);
                println!("  Win rate: {:.2}%", results.metrics.win_rate * 100.0);
                println!("  Profit factor: {:.2}", results.metrics.profit_factor);
                println!("  Total profit: ${:.2}", results.metrics.total_profit);
                println!("  Final balance: ${:.2}", config.initial_balance + results.metrics.total_profit);
                println!("  Max drawdown: {:.2}%", results.metrics.max_drawdown * 100.0);
                println!("  Sharpe ratio: {:.2}", results.metrics.sharpe_ratio);
                println!("  Sortino ratio: {:.2}", results.metrics.sortino_ratio);
                println!("  Risk/reward ratio: {:.2}", results.metrics.risk_reward_ratio);
                
                // Store trade info for this configuration
                all_trades_info.insert(config.name.clone(), get_position_details(&results));
                // ... rest of the function remains the same ...
            },
            Err(e) => {
                println!("Backtest failed: {}", e);
            }
        }
        
        println!("\n--------------------------------------------------");
    }
    
    // Print detailed position information from all strategies
    println!("\n==================== DETAILED POSITION INFORMATION ====================");
    for (strat_name, trade_details) in &all_trades_info {
        println!("\nStrategy: {}", strat_name);
        println!("Trade details (sample):");
        
        // Just show first few trades to avoid overwhelming output
        for (i, detail) in trade_details.iter().take(3).enumerate() {
            println!("{}", detail);
            if i < trade_details.len() - 1 && i < 2 {
                println!("--------------");
            }
        }
        
        if trade_details.len() > 3 {
            println!("... and {} more trades", trade_details.len() - 3);
        }
        
        println!("\n--------------------------------------------------");
    }
    
    println!("\nDebug testing complete.");
    
    Ok(())
}