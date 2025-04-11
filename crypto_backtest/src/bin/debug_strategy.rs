// src/bin/debug_strategy.rs
use std::error::Error;
use crypto_backtest::fetch_data::load_candles_from_csv;
use crypto_backtest::strategy::{Strategy, StrategyConfig};
use crypto_backtest::indicators::{PivotPoints, FibonacciLevels};
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
    
    // Print some sample candles
    println!("\nSample candle data (first 3 records):");
    for (i, candle) in candles.iter().take(3).enumerate() {
        println!("Candle #{}: Time={}, Open={:.2}, High={:.2}, Low={:.2}, Close={:.2}, Volume={:.2}",
            i+1, candle.time, candle.open, candle.high, candle.low, candle.close, candle.volume);
    }
    
    // Create configuration with more permissive parameters
    let config = StrategyConfig {
        initial_balance: 10_000.0,
        leverage: 20.0,
        max_risk_per_trade: 0.01,
        pivot_lookback: 3,         // Smaller lookback for more signals
        signal_lookback: 1,
        fib_threshold: 5.0,        // Reduced threshold to capture more moves
        fib_initial: 0.5,          // More aggressive entry
        fib_tp: 1.618,             // Standard take profit
        fib_sl: 0.382,             // Tighter stop loss
        fib_limit1: 0.618,
        fib_limit2: 1.0,
    };
    
    println!("\nCreating strategy with configuration:");
    println!("  Pivot Lookback: {}", config.pivot_lookback);
    println!("  Fib Threshold: {:.2}", config.fib_threshold);
    println!("  Entry Level: {:.3}", config.fib_initial);
    println!("  Take Profit: {:.3}", config.fib_tp);
    println!("  Stop Loss: {:.3}", config.fib_sl);
    
    // Manually test pivot detection
    let mut pivot_detector = PivotPoints::new(config.pivot_lookback);
    let mut fibonacci = FibonacciLevels::new(
        config.fib_threshold,
        config.fib_initial,
        config.fib_tp,
        config.fib_sl,
        config.fib_limit1,
        config.fib_limit2
    );
    
    println!("\nChecking for pivot points...");
    let mut pivot_highs = Vec::new();
    let mut pivot_lows = Vec::new();
    
    // Process the first X candles to find pivot points
    for (i, candle) in candles.iter().take(100).enumerate() {
        let (pivot_high, pivot_low) = pivot_detector.identify_pivots(candle.high, candle.low);
        
        if let Some(high) = pivot_high {
            pivot_highs.push((i, high));
            println!("Pivot High detected at index {}: {:.2} (time: {})", i, high, candle.time);
        }
        
        if let Some(low) = pivot_low {
            pivot_lows.push((i, low));
            println!("Pivot Low detected at index {}: {:.2} (time: {})", i, low, candle.time);
        }
    }
    
    println!("\nFound {} pivot highs and {} pivot lows in the first 100 candles", 
        pivot_highs.len(), pivot_lows.len());
    
    // Check if we have sufficient pivot points for signal generation
    if pivot_highs.len() >= 2 && pivot_lows.len() >= 2 {
        println!("\nAnalyzing potential trade signals from pivots...");
        
        // Try to generate long signals
        for i in 1..pivot_highs.len() {
            let (prev_idx, prev_high) = pivot_highs[i-1];
            let (curr_idx, curr_high) = pivot_highs[i];
            
            if curr_high > prev_high {
                println!("Potential LONG signal detected: Current high {:.2} > Previous high {:.2}", 
                    curr_high, prev_high);
                
                // Try to find closest pivot low before this high
                let mut closest_low_idx = 0;
                let mut closest_low = 0.0;
                
                for &(low_idx, low_val) in &pivot_lows {
                    if low_idx < curr_idx && (closest_low_idx == 0 || low_idx > closest_low_idx) {
                        closest_low_idx = low_idx;
                        closest_low = low_val;
                    }
                }
                
                if closest_low_idx > 0 {
                    println!("  Found closest pivot low: {:.2} at index {}", closest_low, closest_low_idx);
                    
                    // Calculate Fibonacci levels
                    if let Some(levels) = fibonacci.calculate_long_levels(curr_high, closest_low) {
                        println!("  Fibonacci Levels for Long:");
                        println!("    Entry Price: {:.2}", levels.entry_price);
                        println!("    Take Profit: {:.2}", levels.take_profit);
                        println!("    Stop Loss: {:.2}", levels.stop_loss);
                        println!("    Limit1: {:.2}", levels.limit1);
                        println!("    Limit2: {:.2}", levels.limit2);
                        
                        // Check if any future candles would hit our entry
                        let mut entry_hit = false;
                        let mut tp_hit = false;
                        let mut sl_hit = false;
                        
                        for j in curr_idx+1..candles.len().min(curr_idx+50) {
                            let future_candle = &candles[j];
                            
                            if !entry_hit && future_candle.low <= levels.entry_price && future_candle.high >= levels.entry_price {
                                entry_hit = true;
                                println!("    Entry would be hit at index {} (time: {})", j, future_candle.time);
                            }
                            
                            if entry_hit && !tp_hit && future_candle.high >= levels.take_profit {
                                tp_hit = true;
                                println!("    Take Profit would be hit at index {} (time: {})", j, future_candle.time);
                            }
                            
                            if entry_hit && !sl_hit && future_candle.low <= levels.stop_loss {
                                sl_hit = true;
                                println!("    Stop Loss would be hit at index {} (time: {})", j, future_candle.time);
                            }
                            
                            if entry_hit && (tp_hit || sl_hit) {
                                break;
                            }
                        }
                        
                        if !entry_hit {
                            println!("    Entry price was never hit within the next 50 candles");
                        } else if !tp_hit && !sl_hit {
                            println!("    Neither TP nor SL was hit within the next 50 candles after entry");
                        }
                    } else {
                        println!("  Could not calculate Fibonacci levels (threshold not met)");
                    }
                } else {
                    println!("  Could not find a preceding pivot low");
                }
            }
        }
        
        // Try to generate short signals
        for i in 1..pivot_lows.len() {
            let (prev_idx, prev_low) = pivot_lows[i-1];
            let (curr_idx, curr_low) = pivot_lows[i];
            
            if curr_low < prev_low {
                println!("Potential SHORT signal detected: Current low {:.2} < Previous low {:.2}", 
                    curr_low, prev_low);
                
                // Try to find closest pivot high before this low
                let mut closest_high_idx = 0;
                let mut closest_high = 0.0;
                
                for &(high_idx, high_val) in &pivot_highs {
                    if high_idx < curr_idx && (closest_high_idx == 0 || high_idx > closest_high_idx) {
                        closest_high_idx = high_idx;
                        closest_high = high_val;
                    }
                }
                
                if closest_high_idx > 0 {
                    println!("  Found closest pivot high: {:.2} at index {}", closest_high, closest_high_idx);
                    
                    // Calculate Fibonacci levels
                    if let Some(levels) = fibonacci.calculate_short_levels(closest_high, curr_low) {
                        println!("  Fibonacci Levels for Short:");
                        println!("    Entry Price: {:.2}", levels.entry_price);
                        println!("    Take Profit: {:.2}", levels.take_profit);
                        println!("    Stop Loss: {:.2}", levels.stop_loss);
                        println!("    Limit1: {:.2}", levels.limit1);
                        println!("    Limit2: {:.2}", levels.limit2);
                        
                        // Check if any future candles would hit our entry
                        let mut entry_hit = false;
                        let mut tp_hit = false;
                        let mut sl_hit = false;
                        
                        for j in curr_idx+1..candles.len().min(curr_idx+50) {
                            let future_candle = &candles[j];
                            
                            if !entry_hit && future_candle.low <= levels.entry_price && future_candle.high >= levels.entry_price {
                                entry_hit = true;
                                println!("    Entry would be hit at index {} (time: {})", j, future_candle.time);
                            }
                            
                            if entry_hit && !tp_hit && future_candle.low <= levels.take_profit {
                                tp_hit = true;
                                println!("    Take Profit would be hit at index {} (time: {})", j, future_candle.time);
                            }
                            
                            if entry_hit && !sl_hit && future_candle.high >= levels.stop_loss {
                                sl_hit = true;
                                println!("    Stop Loss would be hit at index {} (time: {})", j, future_candle.time);
                            }
                            
                            if entry_hit && (tp_hit || sl_hit) {
                                break;
                            }
                        }
                        
                        if !entry_hit {
                            println!("    Entry price was never hit within the next 50 candles");
                        } else if !tp_hit && !sl_hit {
                            println!("    Neither TP nor SL was hit within the next 50 candles after entry");
                        }
                    } else {
                        println!("  Could not calculate Fibonacci levels (threshold not met)");
                    }
                } else {
                    println!("  Could not find a preceding pivot high");
                }
            }
        }
    } else {
        println!("\n⚠️ Not enough pivot points detected for signal generation.");
        println!("This could indicate a problem with the pivot point detection algorithm,");
        println!("or that the threshold for detecting pivots is too high.");
    }
    
    println!("\nNow running full strategy backtest...");
    
    // Create strategy and state
    let strategy = Strategy::new(config.clone());
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
    
    // Process all candles
    let mut signals_generated = 0;
    let mut trades_executed = 0;
    
    for (i, candle) in candles.iter().enumerate() {
        // Track pre-analysis state
        let had_position = state.position.is_some();
        let trades_count = state.trades.len();
        
        // Process candle
        let trade_result = strategy.analyze_candle(candle, &mut state);
        
        // Check for position entry
        if !had_position && state.position.is_some() {
            signals_generated += 1;
            
            // Print position details
            let position = state.position.as_ref().unwrap();
            let position_type = match position.position_type {
                PositionType::Long => "Long",
                PositionType::Short => "Short",
            };
            
            println!("\nSignal #{}: {} position opened at index {} (time: {})", 
                signals_generated, position_type, i, candle.time);
            println!("  Entry Price: {:.2}", position.entry_price);
            println!("  Take Profit: {:.2}", position.take_profit);
            println!("  Stop Loss: {:.2}", position.stop_loss);
            println!("  Size: {:.6}", position.size);
            println!("  Risk: {:.2}%", position.risk_percent * 100.0);
            
            if let Some(limit1) = position.limit1_price {
                println!("  Limit1 Price: {:.2}", limit1);
            }
            
            if let Some(limit2) = position.limit2_price {
                println!("  Limit2 Price: {:.2}", limit2);
            }
        }
        
        // Check for trade completion
        if trade_result.is_some() {
            trades_executed += 1;
            let trade = trade_result.unwrap();
            
            println!("\nTrade #{} completed at index {} (time: {})", 
                trades_executed, i, candle.time);
            println!("  Type: {}", trade.position_type);
            println!("  Entry Time: {}", trade.entry_time);
            println!("  Exit Time: {}", trade.exit_time);
            println!("  Entry Price: {:.2}", trade.entry_price);
            println!("  Exit Price: {:.2}", trade.exit_price);
            println!("  Size: {:.6}", trade.size);
            println!("  P&L: ${:.2}", trade.pnl);
            println!("  Current Balance: ${:.2}", state.account_balance);
        }
    }
    
    println!("\nBacktest Summary:");
    println!("  Signals Generated: {}", signals_generated);
    println!("  Trades Executed: {}", trades_executed);
    println!("  Final Balance: ${:.2}", state.account_balance);
    println!("  Total Profit: ${:.2}", state.account_balance - config.initial_balance);
    println!("  Return: {:.2}%", 
        ((state.account_balance - config.initial_balance) / config.initial_balance) * 100.0);
    println!("  Max Drawdown: {:.2}%", state.max_drawdown * 100.0);
    
    Ok(())
}