use std::error::Error;
use std::env;
use std::path::Path;
use std::fs;
use serde_json;

use crypto_backtest::fetch_data::load_candles_from_csv;
use crypto_backtest::optimizer::dynamic_optimizer::{
    DynamicFibonacciOptimizer, 
    DynamicOptimizationConfig, 
    AssetConfig
};

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
        };
        
        // Create the optimizer
        let optimizer = DynamicFibonacciOptimizer::new(opt_config);
        
        // Run the optimization
        println!("Starting dynamic Fibonacci optimization for {}...", symbol);
        let results = optimizer.optimize_asset(symbol, &candles, asset_config.leverage, asset_config.spread)?;
        
        if !results.is_empty() {
            // Run a final backtest with the best parameters
            let best_result = &results[0];
            
            println!("\nBest Parameters:");
            println!("Lookback Period: {}", best_result.lookback_period);
            println!("Initial Level: {:.3}", best_result.initial_level);
            println!("Take Profit Level: {:.3}", best_result.tp_level);
            println!("Stop Loss Level: {:.3}", best_result.sl_level);
            println!("Limit 1 Level: {:.3}", best_result.limit1_level);
            println!("Limit 2 Level: {:.3}", best_result.limit2_level);
            println!("Threshold Factor: {:.2}", best_result.threshold_factor);
            println!("Actual Threshold: {:.2}", best_result.actual_threshold);
            
            println!("\nPerformance:");
            if let Some(trades) = best_result.performance.get("Total Trades") {
                println!("Total Trades: {}", *trades as usize);
            }
            if let Some(win_rate) = best_result.performance.get("Win Rate") {
                println!("Win Rate: {:.2}%", win_rate * 100.0);
            }
            if let Some(profit) = best_result.performance.get("Total Profit") {
                println!("Total Profit: ${:.2}", profit);
            }
            if let Some(drawdown) = best_result.performance.get("Max Drawdown") {
                println!("Max Drawdown: {:.2}%", drawdown * 100.0);
            }
            if let Some(sharpe) = best_result.performance.get("Sharpe Ratio") {
                println!("Sharpe Ratio: {:.2}", sharpe);
            }
            
            // Run final backtest with best parameters
            optimizer.run_final_backtest(symbol, &candles, best_result, asset_config.leverage, asset_config.spread)?;
        } else {
            println!("No valid optimization results found for {}", symbol);
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