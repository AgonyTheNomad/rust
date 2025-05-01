// src/bin/influx_utils.rs
use crypto_backtest::influx::{InfluxConfig, get_candles, get_available_symbols};
use tokio;
use std::fs::File;
use std::io::Write;
use std::error::Error;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    
    if args.len() < 2 {
        println!("Available commands:");
        println!("  symbols   - List all available symbols in the InfluxDB");
        println!("  info      - Show detailed information about candles for a specific symbol");
        println!("  export    - Export candles from InfluxDB to CSV file");
        println!("");
        println!("Examples:");
        println!("  influx_utils symbols");
        println!("  influx_utils info BTC");
        println!("  influx_utils export BTC candles.csv");
        return Ok(());
    }
    
    let command = &args[1];
    
    // Set up InfluxDB connection
    let influx_config = InfluxConfig::default();
    println!("Connecting to InfluxDB: {}", influx_config.url);
    
    match command.as_str() {
        "symbols" => {
            // List all available symbols
            println!("Fetching available symbols from InfluxDB...");
            let symbols = get_available_symbols(&influx_config).await?;
            
            println!("\nFound {} symbols:", symbols.len());
            for (i, symbol) in symbols.iter().enumerate() {
                println!("  {}: {}", i + 1, symbol);
            }
        },
        
        "info" => {
            // Show detailed information about a symbol
            if args.len() < 3 {
                println!("Please provide a symbol. Example: influx_utils info BTC");
                return Ok(());
            }
            
            let symbol = &args[2];
            println!("Fetching candle data for {} from InfluxDB...", symbol);
            
            let candles = get_candles(&influx_config, symbol, None).await?;
            
            if candles.is_empty() {
                println!("No candles found for symbol: {}", symbol);
                return Ok(());
            }
            
            // Calculate statistics
            let start_date = &candles.first().unwrap().time;
            let end_date = &candles.last().unwrap().time;
            let candle_count = candles.len();
            
            // Calculate average and min/max values
            let mut sum_volume = 0.0;
            let mut min_high = f64::MAX;
            let mut max_high = f64::MIN;
            let mut min_low = f64::MAX;
            let mut max_low = f64::MIN;
            let mut open_price = candles.first().unwrap().open;
            let mut close_price = candles.last().unwrap().close;
            
            for candle in &candles {
                sum_volume += candle.volume;
                min_high = min_high.min(candle.high);
                max_high = max_high.max(candle.high);
                min_low = min_low.min(candle.low);
                max_low = max_low.max(candle.low);
            }
            
            let avg_volume = sum_volume / candle_count as f64;
            let price_change = close_price - open_price;
            let price_change_percent = (price_change / open_price) * 100.0;
            
            // Display information
            println!("\nSymbol: {}", symbol);
            println!("Date Range: {} to {}", start_date, end_date);
            println!("Candle Count: {}", candle_count);
            println!("\nPrice Information:");
            println!("  Open:  ${:.2}", open_price);
            println!("  Close: ${:.2}", close_price);
            println!("  Change: ${:.2} ({:.2}%)", price_change, price_change_percent);
            println!("  High Range: ${:.2} - ${:.2}", min_high, max_high);
            println!("  Low Range: ${:.2} - ${:.2}", min_low, max_low);
            println!("\nVolume Information:");
            println!("  Average Volume: {:.2}", avg_volume);
            
            // Display sample candles
            println!("\nSample Candles:");
            println!("First Candle:");
            print_candle(candles.first().unwrap());
            
            println!("\nLast Candle:");
            print_candle(candles.last().unwrap());
            
            // Calculate candles per timeframe
            println!("\nCandle Distribution:");
            // We could implement timeframe analysis here
            println!("  Total candles: {}", candle_count);
        },
        
        "export" => {
            // Export candles to CSV
            if args.len() < 4 {
                println!("Please provide a symbol and output filename.");
                println!("Example: influx_utils export BTC candles.csv");
                return Ok(());
            }
            
            let symbol = &args[2];
            let output_file = &args[3];
            
            println!("Fetching candle data for {} from InfluxDB...", symbol);
            let candles = get_candles(&influx_config, symbol, None).await?;
            
            if candles.is_empty() {
                println!("No candles found for symbol: {}", symbol);
                return Ok(());
            }
            
            println!("Exporting {} candles to {}...", candles.len(), output_file);
            
            // Write to CSV
            let mut writer = csv::Writer::from_path(output_file)?;
            
            // Write header
            writer.write_record(&[
                "Timestamp", "Open", "High", "Low", "Close", "Volume", "NumTrades"
            ])?;
            
            // Write data
            for candle in candles {
                writer.write_record(&[
                    candle.time.clone(),
                    format!("{}", candle.open),
                    format!("{}", candle.high),
                    format!("{}", candle.low),
                    format!("{}", candle.close),
                    format!("{}", candle.volume),
                    format!("{}", candle.num_trades),
                ])?;
            }
            
            writer.flush()?;
            println!("Export complete!");
        },
        
        _ => {
            println!("Unknown command: {}", command);
            println!("Available commands: symbols, info, export");
        }
    }
    
    Ok(())
}

fn print_candle(candle: &crypto_backtest::models::Candle) {
    println!("  Time:   {}", candle.time);
    println!("  Open:   ${:.2}", candle.open);
    println!("  High:   ${:.2}", candle.high);
    println!("  Low:    ${:.2}", candle.low);
    println!("  Close:  ${:.2}", candle.close);
    println!("  Volume: {:.2}", candle.volume);
}