// tests/integration_tests.rs
use crypto_backtest::Candle;
use crypto_backtest::Strategy;
use crypto_backtest::StrategyConfig;
use crypto_backtest::Backtester;
use crypto_backtest::indicators::{PivotPoints, FibonacciLevels};
use std::path::Path;

#[test]
fn test_load_candles_from_csv() {
    // Assuming you have a test CSV file
    let test_file = "tests/test_data/test_candles.csv";
    
    // Skip test if file doesn't exist (for CI environments without test data)
    if !Path::new(test_file).exists() {
        println!("Test file not found, skipping test");
        return;
    }

    let candles = crypto_backtest::fetch_data::load_candles_from_csv(test_file)
        .expect("Failed to load test candles");
    assert!(!candles.is_empty(), "Should load at least one candle");
    
    // Test the first candle's data
    let first_candle = &candles[0];
    assert!(first_candle.open > 0.0, "Open price should be positive");
    assert!(first_candle.volume >= 0.0, "Volume should be non-negative");
}

#[test]
fn test_backtest_with_strategy() {
    // Create a pattern that's likely to trigger a trade
    // Create a pattern with clear pivot points that should trigger a trade
    let mut test_candles = Vec::new();
    
    // Generate 20 candles to ensure we have enough data for lookback periods
    // First, strong uptrend
    for i in 0..5 {
        let base = 100.0 + (i as f64 * 10.0);
        test_candles.push(Candle {
            time: format!("2023-01-{:02}T00:00:00Z", i+1),
            open: base,
            high: base + 10.0,
            low: base - 5.0,
            close: base + 8.0,
            volume: 1000.0 + (i as f64 * 100.0),
            num_trades: 50 + (i * 5),
        });
    }
    
    // Then a pivot/reversal pattern
    test_candles.push(Candle {
        time: "2023-01-06T00:00:00Z".to_string(),
        open: 150.0,
        high: 160.0,  // New high
        low: 145.0,
        close: 147.0, // Close lower - potential reversal
        volume: 2000.0, // Higher volume at pivot
        num_trades: 80,
    });
    
    // Downtrend confirmation
    for i in 0..5 {
        let base = 145.0 - (i as f64 * 8.0);
        test_candles.push(Candle {
            time: format!("2023-01-{:02}T00:00:00Z", i+7),
            open: base + 2.0,
            high: base + 5.0,
            low: base - 5.0,
            close: base - 3.0,
            volume: 1500.0 + (i as f64 * 100.0),
            num_trades: 60 + (i * 3),
        });
    }
    
    // Then another reversal to create clear pivot points
    test_candles.push(Candle {
        time: "2023-01-12T00:00:00Z".to_string(),
        open: 105.0,
        high: 108.0,
        low: 95.0,   // New low
        close: 107.0, // Close higher - potential reversal
        volume: 2200.0, // Higher volume at pivot
        num_trades: 85,
    });
    
    // New uptrend
    for i in 0..5 {
        let base = 110.0 + (i as f64 * 7.0);
        test_candles.push(Candle {
            time: format!("2023-01-{:02}T00:00:00Z", i+13),
            open: base - 2.0,
            high: base + 5.0,
            low: base - 3.0,
            close: base + 4.0,
            volume: 1300.0 + (i as f64 * 100.0),
            num_trades: 65 + (i * 4),
        });
    }
    
    // Configure a strategy with parameters that should trigger on our test data
    let config = StrategyConfig {
        initial_balance: 10_000.0,
        leverage: 5.0,
        max_risk_per_trade: 0.02,          // Higher risk to ensure trade execution
        pivot_lookback: 2,                 // Small lookback for test
        signal_lookback: 1,                // Quick signal generation
        fib_threshold: 5.0,                // Lower threshold to ensure entry trigger
        fib_initial: 0.382,                // Standard Fibonacci entry level
        fib_tp: 0.618,                     // Standard take profit
        fib_sl: 0.236,                     // Standard stop loss
        ..Default::default()
    };

    // Create and run the backtest
    let strategy = Strategy::new(config.clone());
    let mut backtester = Backtester::new(config.initial_balance, strategy);
    
    let results = backtester.run(&test_candles)
        .expect("Backtest should run without errors");
    
    // Specifically verify that at least one trade was executed
    assert!(results.trades.len() > 0, 
            "At least one trade should be executed. Got {} trades.", results.trades.len());
    
    // Print trade details for debugging
    if !results.trades.is_empty() {
        println!("Trade executed:");
        println!("  Entry time: {}", results.trades[0].entry_time);
        println!("  Exit time: {}", results.trades[0].exit_time);
        println!("  Type: {}", results.trades[0].position_type);
        println!("  Entry: ${:.2}", results.trades[0].entry_price);
        println!("  Exit: ${:.2}", results.trades[0].exit_price);
        println!("  PnL: ${:.2}", results.trades[0].pnl);
    }
    
    // Basic assertions about the result
    assert!(results.metrics.total_trades > 0, "Should have executed at least one trade");
    assert!(results.metrics.win_rate >= 0.0 && results.metrics.win_rate <= 1.0, 
            "Win rate should be between 0 and 1");
}

#[test]
fn test_fibonacci_levels() {
    // Test Fibonacci level calculations
    let fib = FibonacciLevels::new(
        10.0,  // threshold
        0.382, // initial_level
        0.618, // tp_level
        0.236, // sl_level
        0.5,   // limit1_level
        0.618  // limit2_level
    );
    
    let prev_high = 110.0;
    let prev_low = 90.0;
    
    // Test long levels
    let long_levels = fib.calculate_long_levels(prev_high, prev_low)
        .expect("Should calculate long levels");
    
    // Entry price should be low + initial_level * range
    let expected_entry = 90.0 + 0.382 * (110.0 - 90.0);
    assert!((long_levels.entry_price - expected_entry).abs() < 0.001, 
            "Long entry price calculation is incorrect");
    
    // Similar tests for other levels
    let expected_tp = 110.0 + 0.618 * (110.0 - 90.0);
    assert!((long_levels.take_profit - expected_tp).abs() < 0.001,
            "Long take profit calculation is incorrect");
}

#[test]
fn test_pivot_points() {
    let mut pivot_detector = PivotPoints::new(2);
    
    // Add some price data
    let (h1, l1) = pivot_detector.identify_pivots(100.0, 90.0);
    let (h2, l2) = pivot_detector.identify_pivots(105.0, 95.0);
    let (h3, l3) = pivot_detector.identify_pivots(110.0, 100.0);
    let (h4, l4) = pivot_detector.identify_pivots(115.0, 105.0);
    let (h5, l5) = pivot_detector.identify_pivots(105.0, 95.0);
    
    // The middle value should be identified as a pivot if it's a local max/min
    assert_eq!(h3, None, "Should not be a pivot yet");
    assert_eq!(l3, None, "Should not be a pivot yet");
    
    // By the time we reach the 5th candle, we should be able to identify pivots
    assert!(h4.is_some() || l4.is_some() || h5.is_some() || l5.is_some(),
            "Should identify at least one pivot after sufficient data");
}