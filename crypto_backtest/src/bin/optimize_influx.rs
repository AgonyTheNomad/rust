// src/bin/optimize_influx.rs
use crypto_backtest::backtest::Backtester;
use crypto_backtest::strategy::{Strategy, StrategyConfig, AssetConfig};
use crypto_backtest::influx::{InfluxConfig, get_candles};
use tokio;
use std::env;
use std::fs::File;
use std::io::Write;
use std::error::Error;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Set up logging
    env_logger::init();
    println!("Starting parameter optimization with InfluxDB data...");
    
    // Get command line arguments
    let args: Vec<String> = env::args().collect();
    
    if args.len() < 2 {
        println!("Usage: optimize_influx <symbol>");
        println!("Example: optimize_influx BTC");
        return Ok(());
    }
    
    let symbol = &args[1];
    let output_dir = format!("optimization_results/{}", symbol);
    
    // Ensure the output directory exists
    std::fs::create_dir_all(&output_dir)?;
    
    // Set up InfluxDB connection
    let influx_config = InfluxConfig::default();
    println!("Connecting to InfluxDB: {}. Bucket: {}", influx_config.url, influx_config.bucket);
    
    // Load candles from InfluxDB
    println!("Loading candle data for {} from InfluxDB...", symbol);
    let mut candles = get_candles(&influx_config, symbol, None).await?;
    
    // Apply data quality filters
    candles.retain(|c| c.volume > 0.0);
    println!("Loaded {} valid candles", candles.len());
    
    if candles.is_empty() {
        return Err("No candle data loaded".into());
    }
    
    // Define parameter ranges for optimization
    let lookback_periods = vec![3, 5, 8, 13];
    let fib_initial_levels = vec![0.236, 0.382, 0.5, 0.618];
    let fib_tp_levels = vec![0.618, 1.0, 1.618, 2.0];
    let fib_sl_levels = vec![0.236, 0.382, 0.5];
    let threshold_factors = vec![5.0, 10.0, 15.0, 20.0];
    
    println!("Starting optimization with:");
    println!("  Lookback periods: {:?}", lookback_periods);
    println!("  Fibonacci entry levels: {:?}", fib_initial_levels);
    println!("  Take profit levels: {:?}", fib_tp_levels);
    println!("  Stop loss levels: {:?}", fib_sl_levels);
    println!("  Threshold factors: {:?}", threshold_factors);
    
    let total_combinations = lookback_periods.len() * fib_initial_levels.len() * 
                             fib_tp_levels.len() * fib_sl_levels.len() * threshold_factors.len();
    
    println!("Total parameter combinations to test: {}", total_combinations);
    
    // Create output file for results
    let results_file = format!("{}/{}_optimization_results.csv", output_dir, symbol);
    let mut writer = csv::Writer::from_path(&results_file)?;
    
    // Write header
    writer.write_record(&[
        "Lookback", "Initial", "TP", "SL", "Threshold", 
        "Total Trades", "Win Rate", "Profit Factor", "Total Profit", 
        "Max Drawdown", "Sharpe Ratio", "Sortino Ratio"
    ])?;
    
    // Create counters for tracking progress
    let mut current_combination = 0;
    let start_time = Instant::now();
    let mut best_profit = 0.0;
    let mut best_params = None;
    
    // Run optimization
    for &lookback in &lookback_periods {
        for &initial in &fib_initial_levels {
            for &tp in &fib_tp_levels {
                for &sl in &fib_sl_levels {
                    for &threshold in &threshold_factors {
                        current_combination += 1;
                        
                        let progress = (current_combination as f64 / total_combinations as f64) * 100.0;
                        let elapsed = start_time.elapsed();
                        
                        // Print progress every 10 combinations
                        if current_combination % 10 == 0 || current_combination == 1 {
                            println!("Testing combination {}/{} ({:.1}%) - Elapsed: {:.2?}", 
                                    current_combination, total_combinations, progress, elapsed);
                        }
                        
                        // Create strategy config with current parameters
                        let config = StrategyConfig {
                            initial_balance: 10_000.0,
                            leverage: 20.0,
                            max_risk_per_trade: 0.01,
                            pivot_lookback: lookback,
                            signal_lookback: 1,
                            fib_threshold: threshold,
                            fib_initial: initial,
                            fib_tp: tp,
                            fib_sl: sl,
                            fib_limit1: 0.5,    // Fixed for optimization
                            fib_limit2: 0.786,  // Fixed for optimization
                        };
                        
                        let asset_config = AssetConfig {
                            name: symbol.to_string(),
                            leverage: 20.0,
                            spread: 0.0005,
                            avg_spread: 0.001,
                        };
                        
                        let strategy = Strategy::new(config.clone(), asset_config);
                        let mut backtester = Backtester::new(config.initial_balance, strategy);
                        
                        match backtester.run(&candles) {
                            Ok(results) => {
                                // Check if we've found a better profit
                                if results.metrics.total_profit > best_profit {
                                    best_profit = results.metrics.total_profit;
                                    best_params = Some((lookback, initial, tp, sl, threshold));
                                    
                                    // Report new best parameters
                                    println!("\nNew best parameters found:");
                                    println!("  Lookback: {}", lookback);
                                    println!("  Initial: {}", initial);
                                    println!("  TP: {}", tp);
                                    println!("  SL: {}", sl);
                                    println!("  Threshold: {}", threshold);
                                    println!("  Profit: ${:.2}", best_profit);
                                }
                                
                                // Write results to CSV
                                writer.write_record(&[
                                    lookback.to_string(),
                                    initial.to_string(),
                                    tp.to_string(),
                                    sl.to_string(),
                                    threshold.to_string(),
                                    results.metrics.total_trades.to_string(),
                                    format!("{:.4}", results.metrics.win_rate),
                                    format!("{:.4}", results.metrics.profit_factor),
                                    format!("{:.2}", results.metrics.total_profit),
                                    format!("{:.4}", results.metrics.max_drawdown),
                                    format!("{:.4}", results.metrics.sharpe_ratio),
                                    format!("{:.4}", results.metrics.sortino_ratio),
                                ])?;
                                
                                // Flush writer every 10 records to save progress
                                if current_combination % 10 == 0 {
                                    writer.flush()?;
                                }
                            },
                            Err(e) => {
                                println!("Error for parameters (Lookback={}, Initial={}, TP={}, SL={}, Threshold={}): {}", 
                                        lookback, initial, tp, sl, threshold, e);
                            }
                        }
                    }
                }
            }
        }
    }
    
    writer.flush()?;
    
    let total_time = start_time.elapsed();
    println!("\nOptimization completed in {:.2?}", total_time);
    println!("Results saved to: {}", results_file);
    
    // Save best parameters to a separate file
    if let Some((lookback, initial, tp, sl, threshold)) = best_params {
        let best_params_file = format!("{}/{}_best_params.json", output_dir, symbol);
        let best_params_json = serde_json::json!({
            "symbol": symbol,
            "parameters": {
                "lookback": lookback,
                "initial": initial,
                "tp": tp,
                "sl": sl,
                "threshold": threshold,
            },
            "performance": {
                "profit": best_profit,
                "return_percent": (best_profit / 10000.0) * 100.0,
            },
            "optimization_info": {
                "total_combinations": total_combinations,
                "duration_seconds": total_time.as_secs(),
            }
        });
        
        let mut file = File::create(&best_params_file)?;
        file.write_all(serde_json::to_string_pretty(&best_params_json)?.as_bytes())?;
        
        println!("Best parameters saved to: {}", best_params_file);
    }
    
    Ok(())
}