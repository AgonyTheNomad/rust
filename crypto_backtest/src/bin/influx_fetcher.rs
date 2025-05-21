// src/bin/influx_fetcher.rs
//
// This standalone binary fetches data from InfluxDB and converts it to CSV files
// that are compatible with your existing backtesting system.

use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use chrono::{DateTime, Utc};
use influxdb::{Client, ReadQuery};
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
            url: "http://127.0.0.1:8086".to_string(),
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
    let read_query = ReadQuery::new(query);
    let response_str = client.query(&read_query).await?;
    
    println!("Query completed, processing results...");
    
    // Parse the response into JSON
    let response_json: Value = serde_json::from_str(&response_str)?;
    
    // Parse the response into Candles
    let mut candles = Vec::new();
    
    // The response should contain series with our query results
    if let Some(series_array) = response_json.get("series").and_then(|s| s.as_array()) {
        for series in series_array {
            // Extract column names
            let columns = series.get("columns")
                .and_then(|c| c.as_array())
                .ok_or("No columns found")?;
            
            let column_names: Vec<String> = columns.iter()
                .filter_map(|c| c.as_str().map(String::from))
                .collect();
            
            let time_index = column_names.iter().position(|c| c == "time")
                .ok_or("No time column found")?;
            let open_index = column_names.iter().position(|c| c == "open")
                .ok_or("No open column found")?;
            let high_index = column_names.iter().position(|c| c == "high")
                .ok_or("No high column found")?;
            let low_index = column_names.iter().position(|c| c == "low")
                .ok_or("No low column found")?;
            let close_index = column_names.iter().position(|c| c == "close")
                .ok_or("No close column found")?;
            let volume_index = column_names.iter().position(|c| c == "volume")
                .ok_or("No volume column found")?;
            let num_trades_index = column_names.iter().position(|c| c == "num_trades")
                .unwrap_or(6); // Optional
            
            // Process each value in the series
            if let Some(values_array) = series.get("values").and_then(|v| v.as_array()) {
                for values in values_array {
                    if let Some(value_array) = values.as_array() {
                        // Parse the timestamp - could be a string or a number
                        let time_str = match &value_array[time_index] {
                            Value::String(s) => s.clone(),
                            Value::Number(n) => n.to_string(),
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
                        let open = value_array[open_index].as_f64().unwrap_or(0.0);
                        let high = value_array[high_index].as_f64().unwrap_or(0.0);
                        let low = value_array[low_index].as_f64().unwrap_or(0.0);
                        let close = value_array[close_index].as_f64().unwrap_or(0.0);
                        let volume = value_array[volume_index].as_f64().unwrap_or(0.0);
                        
                        // num_trades is optional
                        let num_trades = if num_trades_index < value_array.len() {
                            value_array[num_trades_index].as_i64().unwrap_or(0)
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
            }
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
    let read_query = ReadQuery::new(query);
    let response_str = client.query(&read_query).await?;
    
    // Parse the response into JSON
    let response_json: Value = serde_json::from_str(&response_str)?;
    
    // Extract symbols
    let mut symbols = Vec::new();
    
    if let Some(series_array) = response_json.get("series").and_then(|s| s.as_array()) {
        for series in series_array {
            // The series values should contain pairs of (key, value)
            if let Some(values_array) = series.get("values").and_then(|v| v.as_array()) {
                for values in values_array {
                    if let Some(value_array) = values.as_array() {
                        if value_array.len() >= 2 {
                            // Extract the symbol value (second column)
                            if let Some(symbol) = value_array[1].as_str() {
                                symbols.push(symbol.to_string());
                            }
                        }
                    }
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