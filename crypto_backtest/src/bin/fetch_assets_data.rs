// src/bin/fetch_assets_data.rs
use crypto_backtest::influx::{InfluxConfig, get_candles};
use serde_json::Value;
use std::error::Error;
use std::fs;
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Load the assets configuration
    println!("Loading assets configuration...");
    let assets_json = fs::read_to_string("assets.json")?;
    let assets_data: Value = serde_json::from_str(&assets_json)?;
    
    // Extract asset symbols
    let assets = assets_data["assets"].as_array()
        .ok_or("Invalid assets.json format: 'assets' array not found")?;
    
    println!("Found {} assets in configuration", assets.len());
    
    // Create output directory
    let output_dir = "data";
    fs::create_dir_all(output_dir)?;
    
    // Setup InfluxDB connection
    let influx_config = InfluxConfig::default();
    
    // Process each asset
    for (i, asset) in assets.iter().enumerate() {
        let symbol = asset["name"].as_str()
            .ok_or(format!("Missing 'name' field in asset #{}", i+1))?;
        
        println!("[{}/{}] Processing: {}", i+1, assets.len(), symbol);
        
        // Fetch candle data from InfluxDB
        match get_candles(&influx_config, symbol, None).await {
            Ok(candles) => {
                if candles.is_empty() {
                    println!("  No candles found for {}, skipping", symbol);
                    continue;
                }
                
                // Save to CSV file
                let output_file = format!("{}/{}.csv", output_dir, symbol);
                match crypto_backtest::fetch_data::save_candles_to_csv(&candles, &output_file) {
                    Ok(_) => {
                        println!("  Successfully saved {} candles for {} to {}", 
                                candles.len(), symbol, output_file);
                    },
                    Err(e) => {
                        println!("  Error saving candles for {}: {}", symbol, e);
                    }
                }
            },
            Err(e) => {
                println!("  Error fetching candles for {}: {}", symbol, e);
            }
        }
        
        println!("------------------------------------------------");
    }
    
    println!("All assets processed. Data saved to {}", output_dir);
    Ok(())
}