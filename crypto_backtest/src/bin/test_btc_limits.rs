// src/bin/optimize_btc_debug.rs
use std::error::Error;
use crypto_backtest::fetch_data::load_candles_from_csv;
use crypto_backtest::strategy::{Strategy, AssetConfig};
use crypto_backtest::models::{BacktestState, PositionType, default_strategy_config};
use std::fs::File;
use std::io::Write;

// Define parameter combinations to test
struct TestParameters {
    pivot_lookback: usize,
    threshold: f64,
    initial_level: f64,
    tp_level: f64,
    sl_level: f64,
    limit1_level: f64,
    limit2_level: f64,
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

    // Use only the last 1000 candles
    let test_candles = candles.iter()
        .skip(candles.len().saturating_sub(1000)) // Take only the last 1000 candles
        .cloned()
        .collect::<Vec<_>>();
    
    println!("Testing with {} candles from {} to {}", 
        test_candles.len(),
        test_candles.first().map_or("unknown", |c| &c.time),
        test_candles.last().map_or("unknown", |c| &c.time)
    );

    // Define parameter combinations to test
    let parameter_sets = vec![
        // Original parameters that you felt produced too many trades
        TestParameters {
            pivot_lookback: 5,
            threshold: 100.0,
            initial_level: 0.5,
            tp_level: 1.618,
            sl_level: 0.382,
            limit1_level: 0.618,
            limit2_level: 0.786,
        },
        // More conservative parameters (larger threshold, more extreme levels)
        TestParameters {
            pivot_lookback: 8,
            threshold: 250.0,
            initial_level: 0.618,
            tp_level: 2.0,
            sl_level: 0.5,
            limit1_level: 0.786,
            limit2_level: 1.0,
        },
        // Very strict parameters for fewer trades
        TestParameters {
            pivot_lookback: 13,
            threshold: 500.0,
            initial_level: 0.618,
            tp_level: 2.618,
            sl_level: 0.5,
            limit1_level: 0.786,
            limit2_level: 1.0,
        },
    ];

    // Create a file to log parameter test results
    let summary_file = "btc_optimization_summary.csv";
    let mut summary_writer = File::create(summary_file)?;
    writeln!(summary_writer, "ParameterSet,Lookback,Threshold,Initial,TP,SL,Limit1,Limit2,TotalTrades,Limit1Hits,Limit2Hits,TPHits,SLHits,WinRate,FinalBalance,Return")?;

    // Test each parameter set
    for (param_index, params) in parameter_sets.iter().enumerate() {
        println!("\n=============================================");
        println!("Testing Parameter Set #{}:", param_index + 1);
        println!("  Lookback: {}", params.pivot_lookback);
        println!("  Threshold: {:.2}", params.threshold);
        println!("  Initial Level: {:.3}", params.initial_level);
        println!("  Take Profit: {:.3}", params.tp_level);
        println!("  Stop Loss: {:.3}", params.sl_level);
        println!("  Limit1: {:.3}", params.limit1_level);
        println!("  Limit2: {:.3}", params.limit2_level);
        println!("=============================================");

        // Create a strategy configuration
        let mut config = default_strategy_config();
        config.name = format!("BTC Param Set {}", param_index + 1);
        config.leverage = 20.0;
        config.max_risk_per_trade = 0.1; // Lower risk per trade to be safer
        config.pivot_lookback = params.pivot_lookback;
        config.signal_lookback = 1;
        config.fib_threshold = params.threshold;
        config.fib_initial = params.initial_level;
        config.fib_tp = params.tp_level;
        config.fib_sl = params.sl_level;
        config.fib_limit1 = params.limit1_level;
        config.fib_limit2 = params.limit2_level;

        // Create an asset configuration
        let asset_config = AssetConfig {
            name: "BTC".to_string(),
            leverage: 20.0,
            spread: 0.0005,
            avg_spread: 0.001,
        };

        // Create strategy and backtest state
        let mut strategy = Strategy::new(config.clone(), asset_config);
        let mut state = BacktestState {
            account_balance: 10000.0, // Starting with $10,000
            initial_balance: 10000.0,
            position: None,
            equity_curve: vec![10000.0],
            trades: Vec::new(),
            max_drawdown: 0.0,
            peak_balance: 10000.0,
            current_drawdown: 0.0,
        };

        // Create a log file for this parameter set
        let log_file = format!("btc_debug_params_{}.txt", param_index + 1);
        let mut file = File::create(&log_file)?;
        
        // Write header
        writeln!(file, "PARAMETER SET #{}\n", param_index + 1)?;
        writeln!(file, "Lookback: {}", params.pivot_lookback)?;
        writeln!(file, "Threshold: {:.2}", params.threshold)?;
        writeln!(file, "Initial Level: {:.3}", params.initial_level)?;
        writeln!(file, "Take Profit: {:.3}", params.tp_level)?;
        writeln!(file, "Stop Loss: {:.3}", params.sl_level)?;
        writeln!(file, "Limit1: {:.3}", params.limit1_level)?;
        writeln!(file, "Limit2: {:.3}\n", params.limit2_level)?;

        // Process the candles
        let mut position_count = 0;
        let mut limit1_hits = 0;
        let mut limit2_hits = 0;
        let mut tp_hits = 0;
        let mut sl_hits = 0;
        let mut winning_trades = 0;
        let mut losing_trades = 0;

        for (i, candle) in test_candles.iter().enumerate() {
            let had_position = state.position.is_some();
            let old_position = state.position.clone();
            
            // Process candle to generate signals
            match strategy.analyze_candle(candle) {
                Ok(signals) => {
                    for signal in signals {
                        // Create position from signal
                        if let Ok(position) = strategy.create_scaled_position(
                            &signal, 
                            state.account_balance, 
                            config.max_risk_per_trade
                        ) {
                            state.position = Some(position);
                            position_count += 1;
                            
                            // Log the position details
                            let position = state.position.as_ref().unwrap();
                            let detail = format!(
                                "POSITION #{} OPENED\n\
                                Candle #{}: {}\n\
                                Type: {}\n\
                                Entry Price: ${:.2}\n\
                                Stop Loss: ${:.2} ({}% from entry)\n\
                                Take Profit: ${:.2} ({}% from entry)\n\
                                Limit1 Price: ${:.2} ({}% from entry)\n\
                                Limit2 Price: ${:.2} ({}% from entry)\n\
                                Initial Size: {:.8} (${})\n\
                                Limit1 Size: {:.8} (${})\n\
                                Limit2 Size: {:.8} (${})\n\
                                Risk Percent: {:.2}%\n\
                                TP1 after limit1: ${:.2}\n\
                                TP2 after limit2: ${:.2}\n\
                                Account Balance: ${:.2}\n\n",
                                position_count,
                                i, candle.time,
                                if let PositionType::Long = position.position_type { "LONG" } else { "SHORT" },
                                position.entry_price,
                                position.stop_loss, 
                                (1.0 - position.stop_loss / position.entry_price).abs() * 100.0,
                                position.take_profit, 
                                (position.take_profit / position.entry_price - 1.0).abs() * 100.0,
                                position.limit1_price.unwrap_or(0.0), 
                                (1.0 - position.limit1_price.unwrap_or(0.0) / position.entry_price).abs() * 100.0,
                                position.limit2_price.unwrap_or(0.0), 
                                (1.0 - position.limit2_price.unwrap_or(0.0) / position.entry_price).abs() * 100.0,
                                position.size, position.size * position.entry_price,
                                position.limit1_size, position.limit1_size * position.limit1_price.unwrap_or(0.0),
                                position.limit2_size, position.limit2_size * position.limit2_price.unwrap_or(0.0),
                                position.risk_percent * 100.0,
                                position.new_tp1.unwrap_or(0.0),
                                position.new_tp2.unwrap_or(0.0),
                                state.account_balance
                            );
                            file.write_all(detail.as_bytes())?;
                        }
                    }
                }
                Err(e) => {
                    println!("Error analyzing candle: {}", e);
                    continue;
                }
            };

            // Check if we had a position and still have it
            if had_position && state.position.is_some() {
                let position = state.position.as_mut().unwrap();
                let old_pos = old_position.as_ref().unwrap();
                
                // Check for limit order changes
                if position.limit1_hit && !old_pos.limit1_hit {
                    limit1_hits += 1;
                    let detail = format!(
                        "LIMIT1 HIT for position #{}\n\
                        Candle #{}: {}\n\
                        Price: ${:.2}\n\
                        New TP: ${:.2}\n\
                        Account Balance: ${:.2}\n\n",
                        position_count,
                        i, candle.time,
                        position.limit1_price.unwrap_or(0.0),
                        position.new_tp1.unwrap_or(position.take_profit),
                        state.account_balance
                    );
                    file.write_all(detail.as_bytes())?;
                }
                
                if position.limit2_hit && !old_pos.limit2_hit {
                    limit2_hits += 1;
                    let detail = format!(
                        "LIMIT2 HIT for position #{}\n\
                        Candle #{}: {}\n\
                        Price: ${:.2}\n\
                        New TP: ${:.2}\n\
                        Account Balance: ${:.2}\n\n",
                        position_count,
                        i, candle.time,
                        position.limit2_price.unwrap_or(0.0),
                        position.new_tp2.unwrap_or(position.take_profit),
                        state.account_balance
                    );
                    file.write_all(detail.as_bytes())?;
                }
            }
            
            // Check if position was closed
            if had_position && !state.position.is_some() {
                let old_pos = old_position.as_ref().unwrap();
                let pnl = match old_pos.position_type {
                    PositionType::Long => (candle.close - old_pos.entry_price) * old_pos.size,
                    PositionType::Short => (old_pos.entry_price - candle.close) * old_pos.size,
                };
                
                // Determine if it was TP or SL
                let (exit_type, exit_reason) = if candle.high >= old_pos.take_profit && 
                    matches!(old_pos.position_type, PositionType::Long) {
                    tp_hits += 1;
                    winning_trades += 1;
                    ("TAKE PROFIT", "Price reached take profit level")
                } else if candle.low <= old_pos.take_profit && 
                    matches!(old_pos.position_type, PositionType::Short) {
                    tp_hits += 1;
                    winning_trades += 1;
                    ("TAKE PROFIT", "Price reached take profit level")
                } else if candle.low <= old_pos.stop_loss && 
                    matches!(old_pos.position_type, PositionType::Long) {
                    sl_hits += 1;
                    losing_trades += 1;
                    ("STOP LOSS", "Price reached stop loss level")
                } else if candle.high >= old_pos.stop_loss && 
                    matches!(old_pos.position_type, PositionType::Short) {
                    sl_hits += 1;
                    losing_trades += 1;
                    ("STOP LOSS", "Price reached stop loss level")
                } else {
                    if pnl > 0.0 {
                        winning_trades += 1;
                    } else {
                        losing_trades += 1;
                    }
                    ("UNKNOWN EXIT", "Unknown reason for exit")
                };
                
                // Add trade to the state
                let detail = format!(
                    "POSITION #{} CLOSED\n\
                    Candle #{}: {}\n\
                    Exit Type: {}\n\
                    Reason: {}\n\
                    Entry Price: ${:.2}\n\
                    Exit Price: ${:.2}\n\
                    Size: {:.8}\n\
                    PnL: ${:.2} ({:.2}%)\n\
                    Position Held For: {} candles\n\
                    New Account Balance: ${:.2}\n\n",
                    position_count,
                    i, candle.time,
                    exit_type,
                    exit_reason,
                    old_pos.entry_price,
                    candle.close,
                    old_pos.size,
                    pnl,
                    (pnl / (old_pos.entry_price * old_pos.size)) * 100.0,
                    "N/A", // We'd need to track the entry candle index
                    state.account_balance
                );
                file.write_all(detail.as_bytes())?;
            }
        }

        let win_rate = if position_count > 0 {
            winning_trades as f64 / position_count as f64 * 100.0
        } else {
            0.0
        };

        // Write summary to log file
        let summary = format!(
            "\nPARAMETER SET #{} SUMMARY:\n\
            Total positions opened: {}\n\
            Limit1 hits: {} ({:.1}%)\n\
            Limit2 hits: {} ({:.1}%)\n\
            Take profit hits: {} ({:.1}%)\n\
            Stop loss hits: {} ({:.1}%)\n\
            Winning trades: {} ({:.1}%)\n\
            Losing trades: {} ({:.1}%)\n\
            Final balance: ${:.2}\n\
            Total return: ${:.2} ({:.2}%)\n",
            param_index + 1,
            position_count,
            limit1_hits,
            if position_count > 0 { limit1_hits as f64 / position_count as f64 * 100.0 } else { 0.0 },
            limit2_hits,
            if position_count > 0 { limit2_hits as f64 / position_count as f64 * 100.0 } else { 0.0 },
            tp_hits,
            if position_count > 0 { tp_hits as f64 / position_count as f64 * 100.0 } else { 0.0 },
            sl_hits,
            if position_count > 0 { sl_hits as f64 / position_count as f64 * 100.0 } else { 0.0 },
            winning_trades,
            win_rate,
            losing_trades,
            if position_count > 0 { losing_trades as f64 / position_count as f64 * 100.0 } else { 0.0 },
            state.account_balance,
            state.account_balance - 10000.0,
            (state.account_balance - 10000.0) / 10000.0 * 100.0
        );
        file.write_all(summary.as_bytes())?;
        
        println!("{}", summary);
        println!("Detailed log saved to {}", log_file);

        // Add row to summary CSV
        writeln!(summary_writer, "{},{},{},{:.3},{:.3},{:.3},{:.3},{:.3},{},{},{},{},{},{:.1}%,{:.2},{:.2}%",
            param_index + 1,
            params.pivot_lookback,
            params.threshold,
            params.initial_level,
            params.tp_level,
            params.sl_level,
            params.limit1_level,
            params.limit2_level,
            position_count,
            limit1_hits,
            limit2_hits,
            tp_hits,
            sl_hits,
            win_rate,
            state.account_balance,
            (state.account_balance - 10000.0) / 10000.0 * 100.0
        )?;
    }

    summary_writer.flush()?;
    println!("\nOptimization summary saved to {}", summary_file);
    println!("Testing complete! Check the log files for detailed trade information.");

    Ok(())
}