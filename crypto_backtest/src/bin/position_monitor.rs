// src/bin/position_monitor.rs

use std::error::Error;
use crypto_backtest::fetch_data::load_candles_from_csv;
use crypto_backtest::strategy::{Strategy, StrategyConfig, AssetConfig}; // Note the AssetConfig import
use crypto_backtest::models::{BacktestState, PositionType};

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
    let config = StrategyConfig {
        initial_balance: 10_000.0,
        leverage: 20.0,
        max_risk_per_trade: 0.015,
        pivot_lookback: 3,
        signal_lookback: 1,
        fib_threshold: 5.0,
        fib_initial: 0.5,
        fib_tp: 1.618,
        fib_sl: 0.5,
        fib_limit1: 0.618,
        fib_limit2: 1.272,
    };

    println!("\nMonitoring positions with configuration:");
    println!("  Lookback: {}", config.pivot_lookback);
    println!("  Threshold: {:.2}", config.fib_threshold);
    println!("  Initial Level: {:.3}", config.fib_initial);
    println!("  Take Profit: {:.3}", config.fib_tp);
    println!("  Stop Loss: {:.3}", config.fib_sl);
    println!("  Limit1: {:.3}", config.fib_limit1);
    println!("  Limit2: {:.3}", config.fib_limit2);

    // Create an asset configuration.
    // Here we construct one manually using values from your assets.json example.
    let asset_config = AssetConfig {
        name: "BTC".to_string(),
        leverage: 50.0,
        spread: 0.0003782993723669504,
        avg_spread: 0.002266021682225036,
    };

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
        let trade_result = strategy.analyze_candle(candle, &mut state);

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

        if let Some(position) = &state.position {
            let mut limit_hit = false;
            if position.limit1_hit && !position.limit2_hit {
                println!("\n*** LIMIT 1 HIT at candle index {} (time: {}) ***", i, candle.time);
                println!("Limit1 price: ${:.2}", position.limit1_price.unwrap_or(0.0));
                println!("Position size increased by: {:.6}", position.limit1_size);
                println!("New take profit: ${:.2}", position.new_tp1.unwrap_or(position.take_profit));
                println!(
                    "Candle: Open=${:.2}, High=${:.2}, Low=${:.2}, Close=${:.2}",
                    candle.open, candle.high, candle.low, candle.close
                );
                limit_hit = true;
            }
            if position.limit2_hit {
                println!("\n*** LIMIT 2 HIT at candle index {} (time: {}) ***", i, candle.time);
                println!("Limit2 price: ${:.2}", position.limit2_price.unwrap_or(0.0));
                println!("Position size increased by: {:.6}", position.limit2_size);
                println!("New take profit: ${:.2}", position.new_tp2.unwrap_or(position.take_profit));
                println!(
                    "Candle: Open=${:.2}, High=${:.2}, Low=${:.2}, Close=${:.2}",
                    candle.open, candle.high, candle.low, candle.close
                );
                limit_hit = true;
            }
            if limit_hit {
                println!("Current position size: {:.6}", position.size);
            }
        }

        if let Some(trade) = trade_result {
            trade_count += 1;
            let exit_type = match trade.position_type.as_str() {
                "Long" => {
                    if trade.exit_price >= trade.entry_price {
                        "TAKE PROFIT"
                    } else {
                        "STOP LOSS"
                    }
                }
                "Short" => {
                    if trade.exit_price <= trade.entry_price {
                        "TAKE PROFIT"
                    } else {
                        "STOP LOSS"
                    }
                }
                _ => "UNKNOWN",
            };

            println!("\n==================== TRADE #{} COMPLETED ====================", trade_count);
            println!("At candle index {} (time: {})", i, candle.time);
            println!("Position type: {}", trade.position_type);
            println!("Entry time: {}", trade.entry_time);
            println!("Exit time: {}", trade.exit_time);
            println!("Entry price: ${:.2}", trade.entry_price);
            println!("Exit price: ${:.2}", trade.exit_price);
            println!("Exit type: {}", exit_type);
            println!("Position size: {:.6}", trade.size);
            println!("P&L: ${:.2}", trade.pnl);
            println!("Fees: ${:.2}", trade.fees);
            println!("Slippage: ${:.2}", trade.slippage);
            println!("New account balance: ${:.2}", state.account_balance);
            println!("==========================================================");
        }
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
