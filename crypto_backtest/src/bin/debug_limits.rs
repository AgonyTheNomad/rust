// src/bin/debug_limits.rs
use crypto_backtest::backtest::Backtester;
use crypto_backtest::fetch_data::load_candles_from_csv;
use crypto_backtest::strategy::{Strategy, StrategyConfig, AssetConfig};
use crypto_backtest::models::{default_strategy_config, default_asset_config};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    println!("Running limit order debug test...");
    
    // Load data
    let csv_path = "data/BTC_small.csv";
    println!("Loading data from {}...", csv_path);
    let mut candles = load_candles_from_csv(csv_path)?;
    candles.retain(|c| c.volume > 0.0);
    
    println!("Testing with {} candles", candles.len());
    
    // Parameters from your position monitor
    let mut config = default_strategy_config();
    config.leverage = 50.0;
    config.max_risk_per_trade = 0.1;
    config.pivot_lookback = 3;
    config.signal_lookback = 1;
    config.fib_threshold = 5.0;
    config.fib_initial = 0.5;
    config.fib_tp = 1.618;
    config.fib_sl = 0.5;
    config.fib_limit1 = 0.618;
    config.fib_limit2 = 1.272;
    
    // Create an AssetConfig with all required fields
    let asset_config = default_asset_config("BTC");
    
    // Create a custom backtester to track limit hits
    struct LimitTracker {
        last_position: Option<(f64, f64, f64, bool, bool)>, // SL, TP, Size, limit1_hit, limit2_hit
        limit_hits: Vec<(String, f64, f64, f64, f64, f64, bool, bool)>, // Time, Old SL, New SL, Old TP, New TP, Size, limit1, limit2
    }
    
    let mut limit_tracker = LimitTracker {
        last_position: None,
        limit_hits: Vec::new(),
    };
    
    let strategy = Strategy::new(config.clone(), asset_config);
    let mut backtester = Backtester::new(config.initial_balance, strategy);
    
    // Run the backtest
    let results = backtester.run(&candles)?;
    
    // Print trade details
    if !results.trades.is_empty() {
        println!("\nTrade details:");
        for (i, trade) in results.trades.iter().enumerate() {
            println!("Trade #{}: {} from {} to {}", 
                i + 1,
                trade.position_type,
                trade.entry_time,
                trade.exit_time
            );
            println!("  Entry: ${:.2}, Exit: ${:.2}, PnL: ${:.2}", 
                trade.entry_price,
                trade.exit_price,
                trade.pnl
            );
        }
    }
    
    // Next steps to implement this properly:
    println!("\nNext steps to check stop loss movement:");
    println!("1. Update your Strategy::check_exits() method to log position state before/after limit hits");
    println!("2. Add code to check if stop loss is updated when limits are hit");
    println!("3. If stop loss isn't updated currently, modify the code to move the stop loss to a safer level");
    
    // Example code to add to Strategy::check_exits:
    println!("\nHere's some code to add to your Strategy::check_exits() method:");
    println!("```rust");
    println!("// Before checking limit1:");
    println!("let old_sl = position.stop_loss;");
    println!("let old_tp = position.take_profit;");
    println!("let old_size = position.size;");
    println!("");
    println!("// After limit1 is hit but before checking limit2:");
    println!("if hit {{");
    println!("    position.size += position.limit1_size;");
    println!("    position.take_profit = position.new_tp1.unwrap_or(position.take_profit);");
    println!("    // Update stop loss when limit1 is hit");
    println!("    position.stop_loss = position.entry_price; // Move SL to breakeven");
    println!("    position.limit1_hit = true;");
    println!("    println!(\"Limit1 hit: SL moved from ${{}} to ${{}}, TP from ${{}} to ${{}}\",");
    println!("        old_sl, position.stop_loss, old_tp, position.take_profit);");
    println!("}}");
    println!("```");
    
    Ok(())
}