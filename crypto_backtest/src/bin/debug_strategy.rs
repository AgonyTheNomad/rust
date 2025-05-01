// src/bin/debug_strategy.rs
use std::error::Error;
use crypto_backtest::fetch_data::load_candles_from_csv;
use crypto_backtest::strategy::{Strategy, StrategyConfig, AssetConfig};
use crypto_backtest::indicators::{PivotPoints, FibonacciLevels};
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
    
    // Print some sample candles
    println!("\nSample candle data (first 3 records):");
    for (i, candle) in candles.iter().take(3).enumerate() {
        println!("Candle #{}: Time={}, Open={:.2}, High={:.2}, Low={:.2}, Close={:.2}, Volume={:.2}",
            i+1, candle.time, candle.open, candle.high, candle.low, candle.close, candle.volume);
    }
    
    // Create configuration with more permissive parameters
    let mut config = default_strategy_config();
    config.name = "Test Strategy".to_string();
    config.leverage = 20.0;
    config.max_risk_per_trade = 0.01;
    config.pivot_lookback = 3;
    config.signal_lookback = 1;
    config.fib_threshold = 5.0;
    config.fib_initial = 0.5;
    config.fib_tp = 1.618;
    config.fib_sl = 0.382;
    config.fib_limit1 = 0.618;
    config.fib_limit2 = 1.0;
    
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
    
    // ... rest of the function remains the same ...
    
    println!("\nNow running full strategy backtest...");
    
    // Create strategy and state
    let asset_config = default_asset_config("BTC");
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
    
    // Process all candles
    let mut signals_generated = 0;
    let mut trades_executed = 0;
    
    for (i, candle) in candles.iter().enumerate() {
        // Track pre-analysis state
        let had_position = state.position.is_some();
        let trades_count = state.trades.len();
        
        // Process candle - updated for new return type
        let signals = strategy.analyze_candle(candle)?;
        
        // Process signals instead of single trade result
        for signal in signals {
            signals_generated += 1;
            
            // Print signal details
            let signal_type = match signal.position_type {
                PositionType::Long => "Long",
                PositionType::Short => "Short",
            };
            
            println!("\nSignal #{}: {} signal at index {} (time: {})", 
                signals_generated, signal_type, i, candle.time);
            println!("  Entry Price: {:.2}", signal.price);
            println!("  Take Profit: {:.2}", signal.take_profit);
            println!("  Stop Loss: {:.2}", signal.stop_loss);
            println!("  Strength: {:.2}", signal.strength);
            println!("  Reason: {}", signal.reason);
        }
        
        // Track trades if this is how your system works
        // This part depends on your implementation details
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