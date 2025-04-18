// tests/pivot_signal_test.rs
use crypto_backtest::models::{Candle, BacktestState};
use crypto_backtest::strategy::{Strategy, StrategyConfig};
use crypto_backtest::indicators::PivotPoints;

#[test]
fn test_pivot_detection_and_signals() {
    // Create test pivot detector
    let mut pivot_detector = PivotPoints::new(2);
    
    // Create test data with clear pivot patterns
    let test_prices = [
        (100.0, 90.0),   // High, Low for candle 0
        (105.0, 95.0),   // Candle 1
        (110.0, 100.0),  // Candle 2
        (120.0, 105.0),  // Candle 3 - This will be our first pivot high
        (115.0, 100.0),  // Candle 4
        (110.0, 95.0),   // Candle 5
        (105.0, 85.0),   // Candle 6 - This will be our first pivot low
        (110.0, 90.0),   // Candle 7
        (115.0, 95.0),   // Candle 8
        (125.0, 100.0),  // Candle 9 - Second pivot high (higher than first) - should trigger LONG
        (120.0, 95.0),   // Candle 10
        (115.0, 90.0),   // Candle 11
        (110.0, 80.0),   // Candle 12 - Second pivot low (lower than first) - should trigger SHORT
        (115.0, 85.0),   // Candle 13
        (120.0, 90.0),   // Candle 14
    ];
    
    println!("Testing pivot detection only...");
    for (i, &(high, low)) in test_prices.iter().enumerate() {
        let (pivot_high, pivot_low) = pivot_detector.identify_pivots(high, low);
        
        if pivot_high.is_some() || pivot_low.is_some() {
            println!("Candle {}: Pivot High: {:?}, Pivot Low: {:?}", i, pivot_high, pivot_low);
        }
    }
    
    // Reset and test the whole strategy
    println!("\nTesting full strategy with signal generation...");
    
    // Create strategy with smaller lookback for quicker signal generation
    let config = StrategyConfig {
        initial_balance: 10_000.0,
        leverage: 10.0,
        max_risk_per_trade: 0.02,
        pivot_lookback: 2,                // Small lookback for quicker testing
        signal_lookback: 1,               // Minimum lookback
        fib_threshold: 5.0,               // Low threshold to ensure entry conditions are met
        fib_initial: 0.382,
        fib_tp: 0.618,
        fib_sl: 0.236,
        fib_limit1: 0.382,
        fib_limit2: 0.5,
    };
    
    let mut strategy = Strategy::new(config);
    
    // Create backtest state
    let mut state = BacktestState {
        account_balance: 10_000.0,
        initial_balance: 10_000.0,
        position: None,
        equity_curve: vec![10_000.0],
        trades: Vec::new(),
        max_drawdown: 0.0,
        peak_balance: 10_000.0,
        current_drawdown: 0.0,
    };
    
    // Create test candles
    let mut test_candles = Vec::new();
    for (i, &(high, low)) in test_prices.iter().enumerate() {
        test_candles.push(Candle {
            time: format!("2023-01-{:02}T00:00:00Z", i+1),
            open: low + (high - low) * 0.3,  // Just some arbitrary open price
            high,
            low,
            close: low + (high - low) * 0.7, // Just some arbitrary close price
            volume: 1000.0,
            num_trades: 100,
        });
    }
    
    // Process all candles
    let mut long_signals = 0;
    let mut short_signals = 0;
    let mut completed_trades = 0;
    
    // First pass through the original candles to establish positions
    for (i, candle) in test_candles.iter().enumerate() {
        // Store information about previous signals to compare after processing
        let had_long_signal = strategy.is_long_signal();
        let had_short_signal = strategy.is_short_signal();
        
        // Process the candle
        let trade_result = strategy.analyze_candle(candle, &mut state);
        
        // Check if we have signals after processing
        let has_long_signal = strategy.is_long_signal();
        let has_short_signal = strategy.is_short_signal();
        
        // Count signals - only record a signal if it wasn't there before
        if has_long_signal && !had_long_signal {
            println!("Candle {}: LONG SIGNAL GENERATED! Price: {}", i, candle.high);
            long_signals += 1;
        }
        
        if has_short_signal && !had_short_signal {
            println!("Candle {}: SHORT SIGNAL GENERATED! Price: {}", i, candle.low);
            short_signals += 1;
        }
        
        // Check if a trade was completed
        if let Some(trade) = trade_result {
            println!("Candle {}: Trade completed! P&L: {}", i, trade.pnl);
            completed_trades += 1;
        }
        
        // Check position status
        if let Some(pos) = &state.position {
            let position_type = match pos.position_type {
                crypto_backtest::models::PositionType::Long => "Long",
                crypto_backtest::models::PositionType::Short => "Short",
            };
            println!("Candle {}: Active {} position at {}, TP: {}, SL: {}", 
                i, position_type, pos.entry_price, pos.take_profit, pos.stop_loss);
        }
    }
    
    // Extract position details if there's an active position
    let mut take_profit_level = 0.0;
    let mut stop_loss_level = 0.0;
    let mut position_type = "None";
    
    if let Some(pos) = &state.position {
        println!("\nActive position details before extending test:");
        println!("Type: {:?}", pos.position_type);
        println!("Entry price: {}", pos.entry_price);
        println!("Take profit: {}", pos.take_profit);
        println!("Stop loss: {}", pos.stop_loss);
        
        // Store these values for later use, avoiding the borrow
        take_profit_level = pos.take_profit;
        stop_loss_level = pos.stop_loss;
        position_type = match pos.position_type {
            crypto_backtest::models::PositionType::Long => "Long",
            crypto_backtest::models::PositionType::Short => "Short",
        };
        
        // Now create additional candles to trigger trade exit
        println!("\nAdding candles to trigger position exit...");
        
        // Test Take Profit Exit - create a candle with high/low hitting the take profit
        let tp_candle = if position_type == "Long" {
            Candle {
                time: "2023-01-16T00:00:00Z".to_string(),
                open: pos.entry_price,
                high: take_profit_level + 1.0, // Make sure high passes take profit for long
                low: pos.entry_price - 1.0,    // But not hitting stop loss
                close: take_profit_level + 0.5,
                volume: 1000.0,
                num_trades: 100,
            }
        } else {
            Candle {
                time: "2023-01-16T00:00:00Z".to_string(),
                open: pos.entry_price,
                high: pos.entry_price + 1.0,   // Not hitting stop loss
                low: take_profit_level - 1.0,  // Make sure low passes take profit for short
                close: take_profit_level - 0.5,
                volume: 1000.0,
                num_trades: 100,
            }
        };
        
        println!("Adding take profit test candle with high: {}, low: {}", tp_candle.high, tp_candle.low);
        
        // Process the take profit candle
        let trade_result = strategy.analyze_candle(&tp_candle, &mut state);
        
        // Check if the position was closed
        if let Some(trade) = trade_result {
            println!("Position closed on take profit! P&L: {}", trade.pnl);
            completed_trades += 1;
            
            // Verify it was closed as a take profit (exit price should match take profit level)
            assert_eq!(trade.exit_price, take_profit_level, "Position should exit at take profit level");
        } else {
            println!("Error: Position not closed at take profit!");
        }
    } else {
        println!("No active position to test closure");
    }
    
    // Test stop loss - first create a new position
    if state.position.is_none() {
        println!("\nCreating a new position to test stop loss...");
        
        // Add candles that will create another set of pivots and entry signals
        let new_candles = vec![
            Candle {
                time: "2023-01-17T00:00:00Z".to_string(),
                open: 120.0,
                high: 125.0,
                low: 119.0,
                close: 123.0,
                volume: 1000.0,
                num_trades: 100,
            },
            Candle {
                time: "2023-01-18T00:00:00Z".to_string(),
                open: 123.0,
                high: 130.0, // New pivot high
                low: 120.0,
                close: 122.0,
                volume: 1000.0,
                num_trades: 100,
            },
            Candle {
                time: "2023-01-19T00:00:00Z".to_string(),
                open: 122.0,
                high: 124.0,
                low: 118.0,
                close: 119.0,
                volume: 1000.0,
                num_trades: 100,
            },
            Candle {
                time: "2023-01-20T00:00:00Z".to_string(),
                open: 119.0,
                high: 120.0,
                low: 110.0, // New pivot low
                close: 115.0,
                volume: 1000.0,
                num_trades: 100,
            },
        ];
        
        // Process these candles to create new pivots
        for candle in new_candles.iter() {
            strategy.analyze_candle(&candle, &mut state);
        }
        
        // Check if we now have a new position
        if let Some(pos) = &state.position {
            println!("New position created: {:?} at {}", pos.position_type, pos.entry_price);
            println!("Take profit: {}, Stop loss: {}", pos.take_profit, pos.stop_loss);
            
            // Store the stop loss level for assertion
            let stop_loss_to_test = pos.stop_loss;
            let position_is_long = match pos.position_type {
                crypto_backtest::models::PositionType::Long => true,
                crypto_backtest::models::PositionType::Short => false,
            };
            
            // Create a stop loss candle based on position type
            let sl_candle = if position_is_long {
                Candle {
                    time: "2023-01-21T00:00:00Z".to_string(),
                    open: pos.entry_price,
                    high: pos.entry_price + 1.0,
                    low: stop_loss_to_test - 1.0, // Make sure it's below stop loss for long
                    close: stop_loss_to_test - 0.5,
                    volume: 1000.0,
                    num_trades: 100,
                }
            } else {
                Candle {
                    time: "2023-01-21T00:00:00Z".to_string(),
                    open: pos.entry_price,
                    high: stop_loss_to_test + 1.0, // Make sure it's above stop loss for short
                    low: pos.entry_price - 1.0,
                    close: stop_loss_to_test + 0.5,
                    volume: 1000.0,
                    num_trades: 100,
                }
            };
            
            println!("Testing stop loss with candle - High: {}, Low: {}, SL: {}", 
                sl_candle.high, sl_candle.low, stop_loss_to_test);
            
            // Process the stop loss candle
            let trade_result = strategy.analyze_candle(&sl_candle, &mut state);
            
            // Check if the position was closed
            if let Some(trade) = trade_result {
                println!("Position closed on stop loss! P&L: {}", trade.pnl);
                completed_trades += 1;
                
                // Verify it was closed as a stop loss (exit price should match stop loss level)
                assert_eq!(trade.exit_price, stop_loss_to_test, "Position should exit at stop loss level");
            } else {
                println!("Error: Position not closed at stop loss!");
            }
        } else {
            println!("Failed to create a new position for stop loss testing");
        }
    }
    
    println!("\nTest Summary:");
    println!("Long signals detected: {}", long_signals);
    println!("Short signals detected: {}", short_signals);
    println!("Completed trades: {}", completed_trades);
    
    // Assertions
    assert!(long_signals > 0, "Expected at least one long signal");
    assert!(short_signals > 0, "Expected at least one short signal");
    assert!(completed_trades > 0, "Expected at least one completed trade");
    assert!(state.position.is_none(), "Expected no active position at end of test");
}