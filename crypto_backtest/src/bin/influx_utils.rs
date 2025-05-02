// src/bin/influx_utils.rs
use crypto_backtest::influx::{InfluxConfig, get_candles, get_available_symbols};
use std::error::Error;
use std::env;
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    
    if args.len() < 2 {
        println!("Usage:");
        println!("  influx_utils list                        - List available symbols");
        println!("  influx_utils info <symbol>               - Show information about symbol's data");
        println!("  influx_utils export <symbol> [file.csv]  - Export symbol data to CSV");
        return Ok(());
    }
    
    let command = &args[1];
    let influx_config = InfluxConfig::default();
    
    match command.as_str() {
        "list" => {
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
            let open_price = candles.first().unwrap().open;
            let close_price = candles.last().unwrap().close;
            
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
        },
        
        "export" => {
            // Export candles to CSV
            if args.len() < 3 {
                println!("Please provide a symbol.");
                println!("Example: influx_utils export BTC [output.csv]");
                return Ok(());
            }
            
            let symbol = &args[2];
            let output_file = if args.len() >= 4 {
                args[3].clone()
            } else {
                format!("{}.csv", symbol)
            };
            
            println!("Fetching candle data for {} from InfluxDB...", symbol);
            let candles = get_candles(&influx_config, symbol, None).await?;
            
            if candles.is_empty() {
                println!("No candles found for symbol: {}", symbol);
                return Ok(());
            }
            
            println!("Exporting {} candles to {}...", candles.len(), output_file);
            
            // Write to CSV
            crypto_backtest::fetch_data::save_candles_to_csv(&candles, &output_file)?;
            println!("Export complete!");
        },
        
        _ => {
            println!("Unknown command: {}", command);
            println!("Available commands: list, info, export");
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