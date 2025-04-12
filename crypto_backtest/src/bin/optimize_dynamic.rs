// src/bin/optimize_dynamic.rs
use std::error::Error;
use std::env;
use std::path::Path;
use std::fs;
use serde_json;

use crypto_backtest::fetch_data::load_candles_from_csv;
use crypto_backtest::optimizer::dynamic_optimizer::{
    DynamicFibonacciOptimizer, 
    DynamicOptimizationConfig
};
use crypto_backtest::strategy::AssetConfig;

fn main() -> Result<(), Box<dyn Error>> {
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    
    if args.len() < 3 {
        println!("Usage: optimize_dynamic specific <symbol>");
        println!("Example: optimize_dynamic specific BTC");
        return Ok(());
    }
    
    let optimization_type = &args[1];
    let symbol = &args[2];
    
    // Load configuration from file
    let config_path = "optimization_config.json";
    if !Path::new(config_path).exists() {
        return Err(format!("Configuration file not found: {}", config_path).into());
    }
    
    let config_content = fs::read_to_string(config_path)?;
    let json_config: serde_json::Value = serde_json::from_str(&config_content)?;
    
    // Convert JSON configuration to DynamicOptimizationConfig
    let opt_config = DynamicOptimizationConfig {
        initial_balance: json_config["initial_balance"].as_f64().unwrap_or(10000.0),
        drop_threshold: json_config["drop_threshold"].as_f64().unwrap_or(9000.0),
        
        lookback_periods: json_config["lookback_periods"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_u64().map(|i| i as usize)).collect())
            .unwrap_or_else(|| vec![5, 8, 10, 13]),
            
        initial_levels: json_config["initial_levels"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_f64()).collect())
            .unwrap_or_else(|| vec![0.236, 0.382, 0.5, 0.618, 0.786]),
            
        tp_levels: json_config["tp_levels"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_f64()).collect())
            .unwrap_or_else(|| vec![0.618, 1.0, 1.414, 1.618, 2.0, 2.618]),
            
        sl_levels: json_config["sl_levels"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_f64()).collect())
            .unwrap_or_else(|| vec![1.0, 1.618, 2.0, 2.618, 3.618]),
            
        limit1_levels: json_config["limit1_levels"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_f64()).collect())
            .unwrap_or_else(|| vec![0.5, 0.618, 1.0, 1.272]),
            
        limit2_levels: json_config["limit2_levels"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_f64()).collect())
            .unwrap_or_else(|| vec![1.0, 1.272, 1.618, 2.0]),
            
        threshold_factors: json_config["threshold_factors"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_f64()).collect())
            .unwrap_or_else(|| vec![0.75, 1.0, 1.25, 1.5]),
            
        output_dir: json_config["output_dir"].as_str().unwrap_or("results").to_string(),
        parallel: json_config["parallel"].as_bool().unwrap_or(true),
        num_best_results: json_config["num_best_results"].as_u64().unwrap_or(20) as usize,
    };
    
    // Display the loaded configuration
    println!("Loaded optimization configuration:");
    println!("  Lookback periods: {:?}", opt_config.lookback_periods);
    println!("  Initial levels: {:?}", opt_config.initial_levels);
    println!("  Take-profit levels: {:?}", opt_config.tp_levels);
    println!("  Stop-loss levels: {:?}", opt_config.sl_levels);
    println!("  Limit1 levels: {:?}", opt_config.limit1_levels);
    println!("  Limit2 levels: {:?}", opt_config.limit2_levels);
    println!("  Threshold factors: {:?}", opt_config.threshold_factors);
    
    // Get data directory from config
    let data_dir = json_config["data_dir"].as_str().unwrap_or("data");
    
    // Load the market data
    let csv_path = format!("{}/{}.csv", data_dir, symbol);
    println!("Loading data from {}...", csv_path);
    
    if !Path::new(&csv_path).exists() {
        return Err(format!("File not found: {}", csv_path).into());
    }
    
    let mut candles = load_candles_from_csv(&csv_path)?;
    
    // Filter invalid candles
    println!("Loaded {} raw candles", candles.len());
    candles.retain(|c| c.volume > 0.0);
    println!("Filtered to {} valid candles", candles.len());
    
    if candles.is_empty() {
        return Err("No valid candle data loaded".into());
    }
    
    if optimization_type == "specific" {
        // Create an asset configuration for the symbol
        let asset_config = AssetConfig {
            name: symbol.clone(),
            leverage: 50.0,  // Default leverage, could be made configurable
            spread: 0.000005,  // Default spread of 0.05%, could be made configurable
            avg_spread: 0.002266021682225036,
        };
        
        // Try a hardcoded simple config if the normal one doesn't work
        if true {  // Change to false to use your regular config
            println!("\nTrying with a hardcoded test configuration...");
            let test_config = DynamicOptimizationConfig {
                initial_balance: 10_000.0,
                drop_threshold: 9_000.0,
                lookback_periods: vec![5, 10],
                initial_levels: vec![0.382, 0.5],
                tp_levels: vec![1.0, 1.618],
                sl_levels: vec![2.0, 3.0],  // Make sure these are higher than limit2_levels
                limit1_levels: vec![0.5, 0.618],
                limit2_levels: vec![1.0, 1.272],  // Make sure these are higher than limit1_levels
                threshold_factors: vec![1.0, 1.25],
                output_dir: "results".to_string(),
                parallel: true,
                num_best_results: 20,
            };
            
            // Create the optimizer
            let optimizer = DynamicFibonacciOptimizer::new(test_config);
            
            // Custom debugging for base thresholds
            println!("\nCalculating base thresholds for sample lookback periods:");
            for &lookback in &[5, 10] {
                let base_threshold = optimizer.calculate_base_threshold(&candles, lookback);
                println!("  Lookback {}: base threshold = {}", lookback, base_threshold);
                
                // Show some sample combinations that would be created
                println!("  Sample combinations that would be created for lookback {}:", lookback);
                for &threshold_factor in &[1.0, 1.25][..1] {
                    let actual_threshold = base_threshold * threshold_factor;
                    println!("    - With factor {}: actual threshold = {}", threshold_factor, actual_threshold);
                    
                    for &initial in &[0.382, 0.5][..1] {
                        for &tp in &[1.0, 1.618][..1] {
                            for &sl in &[2.0, 3.0][..1] {
                                for &limit1 in &[0.5, 0.618][..1] {
                                    for &limit2 in &[1.0, 1.272][..1] {
                                        // Check if this would be a valid combination
                                        if limit1 >= limit2 || sl <= limit2 {
                                            println!("      ❌ INVALID: initial={}, tp={}, sl={}, limit1={}, limit2={}", 
                                                initial, tp, sl, limit1, limit2);
                                        } else {
                                            println!("      ✓ VALID: initial={}, tp={}, sl={}, limit1={}, limit2={}", 
                                                initial, tp, sl, limit1, limit2);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            
            // Run the optimization with hardcoded values
            let results = optimizer.optimize_asset(symbol, &candles, asset_config.leverage, asset_config.spread)?;
            
            if !results.is_empty() {
                // Run a final backtest with the best parameters
                let best_result = &results[0];
                optimizer.run_final_backtest(symbol, &candles, best_result, asset_config.leverage, asset_config.spread)?;
            } else {
                println!("No valid optimization results found for {}", symbol);
            }
        } else {
            // Create the optimizer with original config
            let optimizer = DynamicFibonacciOptimizer::new(opt_config);
            
            // Run the optimization
            println!("Starting dynamic Fibonacci optimization for {}...", symbol);
            let results = optimizer.optimize_asset(symbol, &candles, asset_config.leverage, asset_config.spread)?;
            
            if !results.is_empty() {
                // Run a final backtest with the best parameters
                let best_result = &results[0];
                optimizer.run_final_backtest(symbol, &candles, best_result, asset_config.leverage, asset_config.spread)?;
            } else {
                println!("No valid optimization results found for {}", symbol);
            }
        }
    } else if optimization_type == "all" {
        // Check if assets.json exists
        let assets_path = "assets.json";
        if !Path::new(assets_path).exists() {
            return Err(format!("Assets file not found: {}", assets_path).into());
        }
        
        // Run the optimization for all assets
        println!("Running optimization for all assets defined in assets.json");
        crypto_backtest::optimizer::dynamic_optimizer::optimize_assets_from_config(assets_path, opt_config)?;
    } else {
        println!("Unknown optimization type: {}", optimization_type);
        println!("Supported types: specific, all");
    }
    
    Ok(())
}