// src/bin/optimize_dynamic.rs
use std::error::Error;
use std::env;
use std::path::Path;

use crypto_backtest::fetch_data::load_candles_from_csv;
use crypto_backtest::optimizer::dynamic_optimizer::{
    DynamicFibonacciOptimizer, 
    load_config_from_file
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
    
    let optimizer = match DynamicFibonacciOptimizer::from_file(config_path) {
        Ok(optimizer) => optimizer,
        Err(e) => {
            println!("Error loading configuration: {}", e);
            println!("Using default configuration instead");
            DynamicFibonacciOptimizer::new(Default::default())
        }
    };
    
    // Get data directory from config
    let data_dir = "data"; // Default value
    
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
        
        // Run the optimization
        println!("Starting optimization for {}...", symbol);
        let results = optimizer.optimize_asset(symbol, &candles, asset_config.leverage, asset_config.spread)?;
        
        if !results.is_empty() {
            // Run a final backtest with the best parameters
            let best_result = &results[0];
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
        crypto_backtest::optimizer::dynamic_optimizer::optimize_assets_from_config(assets_path, optimizer.get_config())?;
    } else {
        println!("Unknown optimization type: {}", optimization_type);
        println!("Supported types: specific, all");
    }
    
    Ok(())
}