// src/bin/position_monitor.rs

use std::error::Error;
use crypto_backtest::fetch_data::load_candles_from_csv;
use crypto_backtest::strategy::{Strategy, StrategyConfig, AssetConfig};
use crypto_backtest::models::{BacktestState, PositionType, default_strategy_config, default_asset_config};

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

    // Create a strategy configuration
    let mut config = default_strategy_config();
    config.name = "Position Monitor".to_string();
    config.leverage = 20.0;
    config.max_risk_per_trade = 0.015;
    config.pivot_lookback = 3;
    config.signal_lookback = 1;
    config.fib_threshold = 5.0;
    config.fib_initial = 0.5;
    config.fib_tp = 1.618;
    config.fib_sl = 0.5;
    config.fib_limit1 = 0.618;
    config.fib_limit2 = 1.272;

    println!("\nMonitoring positions with configuration:");
    println!("  Lookback: {}", config.pivot_lookback);
    println!("  Threshold: {:.2}", config.fib_threshold);
    println!("  Initial Level: {:.3}", config.fib_initial);
    println!("  Take Profit: {:.3}", config.fib_tp);
    println!("  Stop Loss: {:.3}", config.fib_sl);
    println!("  Limit1: {:.3}", config.fib_limit1);
    println!("  Limit2: {:.3}", config.fib_limit2);

    // Create an asset configuration
    let asset_config = default_asset_config("BTC");

    // Create strategy and backtest state.
    let mut strategy = Strategy::new(config.clone(), asset_config);
    let mut state = BacktestState {
        account_balance: config.initial_balance,
        initial_balance: config.initial_balance,
        position: None,
        equity_curve: vec![config.initial_balance],
        trades: Vec::new(),
        max_drawdown: 0.0,
        peak_balance: config.initial_balance,
        current_drawdown: 0.0,
    };

    println!("\nStarting position monitoring...");

    // Process the last 5000 candles in chronological order.
    let last_candles: Vec<_> = candles.iter().rev().take(5000).collect();
    let last_candles: Vec<_> = last_candles.into_iter().rev().collect();
    println!("Processing last {} candles...", last_candles.len());

    let mut position_count = 0;
    let mut trade_count = 0;

    for (i, candle) in last_candles.iter().enumerate() {
        let had_position = state.position.is_some();
        
        // Process candle - updated for new return type
        match strategy.analyze_candle(candle) {
            Ok(signals) => {
                // Handle signals as needed based on your implementation
                for signal in signals {
                    // Process each signal
                    // This will depend on your implementation of how signals become trades
                }
            }
            Err(e) => {
                println!("Error analyzing candle: {}", e);
                continue;
            }
        };

        // Check if we got a new position
        if !had_position && state.position.is_some() {
            position_count += 1;
            let position = state.position.as_ref().unwrap();
            let position_type = match position.position_type {
                PositionType::Long => "Long",
                PositionType::Short => "Short",
            };

            println!("\n==================== POSITION #{} OPENED ====================", position_count);
            println!("At candle index {} (time: {})", i, candle.time);
            println!("Position type: {}", position_type);
            println!("Entry price: ${:.2}", position.entry_price);
            println!("Take profit: ${:.2}", position.take_profit);
            println!("Stop loss: ${:.2}", position.stop_loss);

            println!("Limit1 price: {}", position.limit1_price.map_or("Not set".to_string(), |v| format!("${:.2}", v)));
            println!("Limit2 price: {}", position.limit2_price.map_or("Not set".to_string(), |v| format!("${:.2}", v)));

            println!("Initial position size: {:.6}", position.size);
            println!("Limit1 size: {:.6}", position.limit1_size);
            println!("Limit2 size: {:.6}", position.limit2_size);

            println!("Risk percent: {:.2}%", position.risk_percent * 100.0);
            println!("Margin used: {:.6}", position.margin_used);

            if let Some(new_tp1) = position.new_tp1 {
                println!("New TP after limit1 hit: ${:.2}", new_tp1);
            }
            if let Some(new_tp2) = position.new_tp2 {
                println!("New TP after limit2 hit: ${:.2}", new_tp2);
            }
            println!(
                "Candle at entry: Open=${:.2}, High=${:.2}, Low=${:.2}, Close=${:.2}",
                candle.open, candle.high, candle.low, candle.close
            );
            println!("==========================================================");
        }

        // ... rest of the existing logic ...
    }

    println!("\nPosition monitoring complete.");
    println!("Total positions opened: {}", position_count);
    println!("Total trades completed: {}", trade_count);
    println!("Final account balance: ${:.2}", state.account_balance);
    println!(
        "Total return: ${:.2} ({:.2}%)",
        state.account_balance - config.initial_balance,
        (state.account_balance - config.initial_balance) / config.initial_balance * 100.0
    );

    Ok(())
}