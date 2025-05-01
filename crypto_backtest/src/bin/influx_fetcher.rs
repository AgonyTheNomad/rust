// src/bin/influx_fetcher.rs
//
// This standalone binary fetches data from InfluxDB and converts it to CSV files
// that are compatible with your existing backtesting system.

use std::env;
use std::error::Error;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use chrono::{DateTime, Utc};
use influxdb::{Client, ReadQuery};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Candle {
    time: String,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
    num_trades: i64,
}

struct InfluxConfig {
    url: String,
    database: String,
}

impl InfluxConfig {
    fn default() -> Self {
        Self {
            url: "http://192.168.68.52:30086".to_string(),
            database: "hyper_candles".to_string(),
        }
    }
}

async fn get_candles(config: &InfluxConfig, symbol: &str, days_back: i64) -> Result<Vec<Candle>, Box<dyn Error>> {
    println!("Connecting to InfluxDB at {}", config.url);
    
    // Create client
    let client = Client::new(config.url.clone(), config.database.clone());
    
    // Build the query - with the older influxdb crate, we use InfluxQL instead of Flux
    let query = format!(
        "SELECT open, high, low, close, volume, num_trades FROM candles WHERE symbol='{}' AND time > now() - {}d",
        symbol, days_back
    );
    
    println!("Executing query: {}", query);
    
    // Execute the query
    let response = client.query(&ReadQuery::new(query)).await?;
    
    println!("Query completed, processing results...");
    
    // Parse the response into Candles
    let mut candles = Vec::new();
    
    // The response should contain series with our query results
    for series in response.series {
        let time_index = series.columns.iter().position(|c| c == "time")
            .ok_or("No time column found")?;
        let open_index = series.columns.iter().position(|c| c == "open")
            .ok_or("No open column found")?;
        let high_index = series.columns.iter().position(|c| c == "high")
            .ok_or("No high column found")?;
        let low_index = series.columns.iter().position(|c| c == "low")
            .ok_or("No low column found")?;
        let close_index = series.columns.iter().position(|c| c == "close")
            .ok_or("No close column found")?;
        let volume_index = series.columns.iter().position(|c| c == "volume")
            .ok_or("No volume column found")?;
        let num_trades_index = series.columns.iter().position(|c| c == "num_trades")
            .unwrap_or(0); // Optional
        
        // Process each value in the series
        for values in series.values {
            // Parse the timestamp - could be a string or a number
            let time_str = match &values[time_index] {
                influxdb::Value::String(s) => s.clone(),
                influxdb::Value::Integer(i) => i.to_string(),
                _ => return Err("Unexpected time format".into()),
            };
            
            // Convert timestamp to a standard format
            let time = if let Ok(dt) = time_str.parse::<DateTime<Utc>>() {
                dt.to_rfc3339()
            } else {
                // If parsing fails, just use the original string
                time_str
            };
            
            // Extract numerical values
            let open = match &values[open_index] {
                influxdb::Value::Float(f) => *f,
                influxdb::Value::Integer(i) => *i as f64,
                _ => return Err("Unexpected open format".into()),
            };
            
            let high = match &values[high_index] {
                influxdb::Value::Float(f) => *f,
                influxdb::Value::Integer(i) => *i as f64,
                _ => return Err("Unexpected high format".into()),
            };
            
            let low = match &values[low_index] {
                influxdb::Value::Float(f) => *f,
                influxdb::Value::Integer(i) => *i as f64,
                _ => return Err("Unexpected low format".into()),
            };
            
            let close = match &values[close_index] {
                influxdb::Value::Float(f) => *f,
                influxdb::Value::Integer(i) => *i as f64,
                _ => return Err("Unexpected close format".into()),
            };
            
            let volume = match &values[volume_index] {
                influxdb::Value::Float(f) => *f,
                influxdb::Value::Integer(i) => *i as f64,
                _ => return Err("Unexpected volume format".into()),
            };
            
            // num_trades is optional
            let num_trades = if num_trades_index > 0 {
                match &values[num_trades_index] {
                    influxdb::Value::Float(f) => *f as i64,
                    influxdb::Value::Integer(i) => *i,
                    _ => 0,
                }
            } else {
                0
            };
            
            // Create and add the candle
            let candle = Candle {
                time,
                open,
                high,
                low,
                close,
                volume,
                num_trades,
            };
            
            candles.push(candle);
        }
    }
    
    println!("Loaded {} candles from InfluxDB", candles.len());
    
    // Sort candles by time
    candles.sort_by(|a, b| a.time.cmp(&b.time));
    
    Ok(candles)
}

async fn get_available_symbols(config: &InfluxConfig) -> Result<Vec<String>, Box<dyn Error>> {
    let client = Client::new(config.url.clone(), config.database.clone());
    
    // Query to get distinct symbols
    let query = "SHOW TAG VALUES FROM candles WITH KEY = \"symbol\"";
    
    // Execute the query
    let response = client.query(&ReadQuery::new(query)).await?;
    
    // Extract symbols
    let mut symbols = Vec::new();
    
    for series in response.series {
        // The series values should contain pairs of (key, value)
        for values in series.values {
            if values.len() >= 2 {
                // Extract the symbol value (second column)
                if let influxdb::Value::String(symbol) = &values[1] {
                    symbols.push(symbol.clone());
                }
            }
        }
    }
    
    println!("Found {} symbols in InfluxDB", symbols.len());
    
    Ok(symbols)
}

async fn save_candles_to_csv(candles: &[Candle], file_path: &str) -> Result<(), Box<dyn Error>> {
    // Create parent directories if needed
    if let Some(parent) = Path::new(file_path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    
    let mut writer = csv::Writer::from_path(file_path)?;
    
    // Write header
    writer.write_record(&["Timestamp", "Open", "High", "Low", "Close", "Volume", "NumTrades"])?;
    
    // Write data
    for candle in candles {
        writer.write_record(&[
            &candle.time,
            &candle.open.to_string(),
            &candle.high.to_string(),
            &candle.low.to_string(),
            &candle.close.to_string(),
            &candle.volume.to_string(),
            &candle.num_trades.to_string(),
        ])?;
    }
    
    writer.flush()?;
    println!("Successfully saved {} candles to CSV: {}", candles.len(), file_path);
    
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    
    if args.len() < 2 {
        println!("Usage:");
        println!("  influx_fetcher list                        - List available symbols");
        println!("  influx_fetcher fetch <symbol> <output.csv> - Fetch a specific symbol");
        println!("  influx_fetcher fetch-all <output_dir>      - Fetch all available symbols");
        return Ok(());
    }
    
    let command = &args[1];
    let influx_config = InfluxConfig::default();
    let days_back = 365; // Default to fetching 1 year of data
    
    match command.as_str() {
        "list" => {
            // List all available symbols
            let symbols = get_available_symbols(&influx_config).await?;
            println!("\nAvailable symbols:");
            for (i, symbol) in symbols.iter().enumerate() {
                println!("  {}: {}", i + 1, symbol);
            }
        },
        
        "fetch" => {
            if args.len() < 4 {
                println!("Please provide a symbol and output filename.");
                println!("Example: influx_fetcher fetch BTC output.csv");
                return Ok(());
            }
            
            let symbol = &args[2];
            let output_file = &args[3];
            
            // Fetch candles
            let candles = get_candles(&influx_config, symbol, days_back).await?;
            
            if candles.is_empty() {
                println!("No candles found for symbol: {}", symbol);
                return Ok(());
            }
            
            // Save to CSV
            save_candles_to_csv(&candles, output_file).await?;
            println!("Candles for {} successfully saved to {}", symbol, output_file);
        },
        
        "fetch-all" => {
            if args.len() < 3 {
                println!("Please provide an output directory.");
                println!("Example: influx_fetcher fetch-all data");
                return Ok(());
            }
            
            let output_dir = &args[2];
            
            // Create output directory
            std::fs::create_dir_all(output_dir)?;
            
            // Get all symbols
            let symbols = get_available_symbols(&influx_config).await?;
            println!("Found {} symbols to process", symbols.len());
            
            // Process each symbol
            for (i, symbol) in symbols.iter().enumerate() {
                println!("[{}/{}] Processing: {}", i + 1, symbols.len(), symbol);
                
                let output_file = format!("{}/{}.csv", output_dir, symbol);
                
                match get_candles(&influx_config, symbol, days_back).await {
                    Ok(candles) => {
                        if candles.is_empty() {
                            println!("  No candles found for {}, skipping", symbol);
                            continue;
                        }
                        
                        // Save to CSV
                        if let Err(e) = save_candles_to_csv(&candles, &output_file).await {
                            println!("  Error saving candles for {}: {}", symbol, e);
                        } else {
                            println!("  Successfully saved {} candles for {} to {}", 
                                     candles.len(), symbol, output_file);
                        }
                    },
                    Err(e) => {
                        println!("  Error fetching candles for {}: {}", symbol, e);
                    }
                }
                
                println!("------------------------------------------------");
            }
            
            println!("All symbols processed. Data saved to {}", output_dir);
        },
        
        _ => {
            println!("Unknown command: {}", command);
            println!("Available commands: list, fetch, fetch-all");
        }
    }
    
    Ok(())
}