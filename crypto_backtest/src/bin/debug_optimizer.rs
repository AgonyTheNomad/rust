// src/bin/debug_optimizer.rs
use std::error::Error;
use std::path::Path;

use crypto_backtest::fetch_data::load_candles_from_csv;
use crypto_backtest::strategy::{Strategy, StrategyConfig};
use crypto_backtest::backtest::Backtester;

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
        StrategyConfig {
            initial_balance: 10_000.0,
            leverage: 20.0,
            max_risk_per_trade: 0.01,
            pivot_lookback: 5,
            signal_lookback: 1,
            fib_threshold: 10.0,
            fib_initial: 0.382,
            fib_tp: 1.0,
            fib_sl: 0.5,
            fib_limit1: 0.5,
            fib_limit2: 1.0,
        },
        // Test config 2: Aggressive
        StrategyConfig {
            initial_balance: 10_000.0,
            leverage: 50.0,
            max_risk_per_trade: 0.02,
            pivot_lookback: 3,  // Smaller lookback for more signals
            signal_lookback: 1,
            fib_threshold: 5.0,  // Lower threshold to catch more moves
            fib_initial: 0.5,
            fib_tp: 1.618,
            fib_sl: 0.382,
            fib_limit1: 0.618,
            fib_limit2: 1.0,
        },
        // Test config 3: Another variation
        StrategyConfig {
            initial_balance: 10_000.0,
            leverage: 30.0,
            max_risk_per_trade: 0.015,
            pivot_lookback: 8,
            signal_lookback: 2,  // Increased for more confirmation
            fib_threshold: 15.0,
            fib_initial: 0.618,
            fib_tp: 2.0,
            fib_sl: 0.618,
            fib_limit1: 0.786,
            fib_limit2: 1.272,
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
        
        // Create a copy of the strategy for monitoring
        let strat_name = format!("Config_{}", i+1);
        let strategy = Strategy::new(config.clone());
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
                all_trades_info.insert(strat_name.clone(), get_position_details(&results));
                
                if results.trades.len() > 0 {
                    println!("\nTrade Details:");
                    println!("  First Trade:");
                    let first_trade = &results.trades[0];
                    println!("    Entry Time: {}", first_trade.entry_time);
                    println!("    Exit Time: {}", first_trade.exit_time);
                    println!("    Type: {}", first_trade.position_type);
                    println!("    Entry Price: ${:.2}", first_trade.entry_price);
                    println!("    Exit Price: ${:.2}", first_trade.exit_price);
                    println!("    Size: {:.6}", first_trade.size);
                    println!("    P&L: ${:.2}", first_trade.pnl);
                    println!("    Risk %: {:.2}%", first_trade.risk_percent * 100.0);
                    println!("    Fees: ${:.2}", first_trade.fees);
                    println!("    Slippage: ${:.2}", first_trade.slippage);
                    
                    if results.trades.len() > 1 {
                        println!("\n  Last Trade:");
                        let last_trade = &results.trades[results.trades.len() - 1];
                        println!("    Entry Time: {}", last_trade.entry_time);
                        println!("    Exit Time: {}", last_trade.exit_time);
                        println!("    Type: {}", last_trade.position_type);
                        println!("    Entry Price: ${:.2}", last_trade.entry_price);
                        println!("    Exit Price: ${:.2}", last_trade.exit_price);
                        println!("    Size: {:.6}", last_trade.size);
                        println!("    P&L: ${:.2}", last_trade.pnl);
                        println!("    Risk %: {:.2}%", last_trade.risk_percent * 100.0);
                        println!("    Fees: ${:.2}", last_trade.fees);
                        println!("    Slippage: ${:.2}", last_trade.slippage);
                    }
                    
                    // Analyze trade types and performance
                    let long_trades = results.trades.iter()
                        .filter(|t| t.position_type == "Long")
                        .count();
                    let short_trades = results.trades.iter()
                        .filter(|t| t.position_type == "Short")
                        .count();
                    let profitable_trades = results.trades.iter()
                        .filter(|t| t.pnl > 0.0)
                        .count();
                    let losing_trades = results.trades.iter()
                        .filter(|t| t.pnl <= 0.0)
                        .count();
                    
                    println!("\n  Trade Distribution:");
                    println!("    Long Trades: {} ({:.1}%)", 
                        long_trades, 
                        (long_trades as f64 / results.trades.len() as f64) * 100.0);
                    println!("    Short Trades: {} ({:.1}%)", 
                        short_trades, 
                        (short_trades as f64 / results.trades.len() as f64) * 100.0);
                    println!("    Profitable Trades: {} ({:.1}%)", 
                        profitable_trades, 
                        (profitable_trades as f64 / results.trades.len() as f64) * 100.0);
                    println!("    Losing Trades: {} ({:.1}%)", 
                        losing_trades, 
                        (losing_trades as f64 / results.trades.len() as f64) * 100.0);
                    
                    // Calculate average P&L
                    let total_pnl: f64 = results.trades.iter().map(|t| t.pnl).sum();
                    let avg_pnl = total_pnl / results.trades.len() as f64;
                    
                    let avg_win = results.trades.iter()
                        .filter(|t| t.pnl > 0.0)
                        .map(|t| t.pnl)
                        .sum::<f64>() / profitable_trades.max(1) as f64;
                    
                    let avg_loss = results.trades.iter()
                        .filter(|t| t.pnl <= 0.0)
                        .map(|t| t.pnl.abs())
                        .sum::<f64>() / losing_trades.max(1) as f64;
                    
                    println!("\n  P&L Statistics:");
                    println!("    Average P&L: ${:.2}", avg_pnl);
                    println!("    Average Win: ${:.2}", avg_win);
                    println!("    Average Loss: ${:.2}", avg_loss);
                    println!("    Win/Loss Ratio: {:.2}", avg_win / avg_loss.max(0.01));
                    
                    // Exit types analysis
                    let trades_with_tp = results.trades.iter()
                        .filter(|t| {
                            let is_long = t.position_type == "Long";
                            let is_tp = if is_long { t.exit_price >= t.entry_price } else { t.exit_price <= t.entry_price };
                            is_tp
                        })
                        .count();
                    
                    let trades_with_sl = results.trades.len() - trades_with_tp;
                    
                    println!("\n  Exit Types:");
                    println!("    Take Profit Exits: {} ({:.1}%)", 
                        trades_with_tp, 
                        (trades_with_tp as f64 / results.trades.len() as f64) * 100.0);
                    println!("    Stop Loss Exits: {} ({:.1}%)", 
                        trades_with_sl, 
                        (trades_with_sl as f64 / results.trades.len() as f64) * 100.0);
                    
                } else {
                    println!("\n⚠️ NO TRADES WERE EXECUTED WITH THIS CONFIGURATION ⚠️");
                }
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