// src/bin/signal_generator.rs
use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use clap::Parser;
use log::*;
use rust_trader::{
    influxdb::{InfluxDBClient, InfluxDBConfig},
    models::{Candle, Signal, PositionType},
    setup_logging,
    signals::{fibonacci::FibonacciLevels, pivots::PivotPoints},
    strategy::{Strategy, StrategyConfig, AssetConfig},
};
use serde_json;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration as StdDuration;
use tokio::time;

// CLI Arguments
#[derive(Parser)]
#[clap(author, version, about = "Signal Generator for Trading Strategy")]
struct Args {
    /// Path to configuration file
    #[clap(short, long, default_value = "config/trader.toml")]
    config: PathBuf,
    
    /// Symbols to analyze (comma separated)
    #[clap(short, long)]
    symbols: Option<String>,
    
    /// Output directory for signals
    #[clap(short, long, default_value = "signals")]
    output: PathBuf,
    
    /// Interval in seconds between updates
    #[clap(short, long, default_value = "60")]
    interval: u64,
}

// Configuration structure
#[derive(serde::Deserialize)]
struct Config {
    general: GeneralConfig,
    influxdb: InfluxDBConfig,
    strategy: StrategyConfig,
    assets: HashMap<String, AssetConfig>,
}

#[derive(serde::Deserialize)]
struct GeneralConfig {
    refresh_interval: u64,  // seconds
    data_dir: PathBuf,
    log_level: String,
    max_candles: usize,
    historical_days: u32,
}

// Load configuration from TOML file
fn load_config<P: AsRef<std::path::Path>>(path: P) -> Result<Config> {
    let mut file = File::open(path).context("Failed to open config file")?;
    let mut contents = String::new();
    std::io::Read::read_to_string(&mut file, &mut contents).context("Failed to read config file")?;
    let config: Config = toml::from_str(&contents).context("Failed to parse config file")?;
    Ok(config)
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line args
    let args = Args::parse();
    
    // Set up logging
    setup_logging();
    
    // Load configuration
    let config = load_config(&args.config)?;
    
    // Create output directory if it doesn't exist
    std::fs::create_dir_all(&args.output)?;
    
    // Connect to InfluxDB
    let influx_client = InfluxDBClient::new(config.influxdb)?;
    
    // Get list of symbols to analyze
    let symbols = match args.symbols {
        Some(list) => list.split(',').map(|s| s.trim().to_uppercase()).collect::<Vec<_>>(),
        None => {
            let symbols_from_db = influx_client.get_symbols().await?;
            if symbols_from_db.is_empty() {
                return Err(anyhow::anyhow!("No symbols found in InfluxDB"));
            }
            symbols_from_db
        }
    };
    
    info!("Analyzing {} symbols: {}", symbols.len(), symbols.join(", "));
    
    // Create a map to track last update time and strategy instance for each symbol
    let mut symbol_states: HashMap<String, (Strategy, DateTime<Utc>)> = HashMap::new();
    
    // Initialize with historical data
    for symbol in &symbols {
        let now = Utc::now();
        let start_time = now - Duration::days(config.general.historical_days as i64);
        
        info!("Loading historical data for {} from {} to {}", 
            symbol, start_time.to_rfc3339(), now.to_rfc3339());
        
        let candles = influx_client.get_candles(symbol, &start_time, &now).await?;
        
        if candles.is_empty() {
            warn!("No candles found for {}", symbol);
            continue;
        }
        
        info!("Loaded {} candles for {}", candles.len(), symbol);
        
        // Create strategy for this symbol
        let asset_config = match config.assets.get(symbol) {
            Some(asset) => asset.clone(),
            None => {
                warn!("No asset config found for {}, using default values", symbol);
                AssetConfig {
                    name: symbol.to_string(),
                    leverage: 20.0,
                    spread: 0.0005,
                    avg_spread: 0.001,
                }
            }
        };
        
        let mut strategy = Strategy::new(config.strategy.clone(), asset_config);
        
        // Initialize with historical data
        strategy.initialize_with_history(&candles)?;
        
        // Store the strategy and last update time
        let last_update = match candles.last() {
            Some(last_candle) => {
                DateTime::parse_from_rfc3339(&last_candle.time)
                    .map_err(|e| anyhow::anyhow!("Failed to parse candle time: {}", e))?
                    .with_timezone(&Utc)
            },
            None => now,
        };
        
        symbol_states.insert(symbol.clone(), (strategy, last_update));
    }
    
    // Main loop - run continuously
    let mut interval = time::interval(StdDuration::from_secs(args.interval));
    
    loop {
        interval.tick().await;
        
        for (symbol, (strategy, last_update)) in &mut symbol_states {
            // Get new candles since last update
            let now = Utc::now();
            let new_candles = influx_client.get_candles(symbol, last_update, &now).await?;
            
            if !new_candles.is_empty() {
                info!("Processing {} new candles for {}", new_candles.len(), symbol);
                
                // Process each new candle
                for candle in &new_candles {
                    // Generate signals for this candle
                    let signals = strategy.analyze_candle(candle)?;
                    
                    // Output any signals
                    if !signals.is_empty() {
                        for signal in &signals {
                            output_signal(signal, &args.output).await?;
                        }
                    }
                }
                
                // Update last processed time
                if let Some(latest_candle) = new_candles.last() {
                    *last_update = DateTime::parse_from_rfc3339(&latest_candle.time)
                        .map_err(|e| anyhow::anyhow!("Failed to parse candle time: {}", e))?
                        .with_timezone(&Utc);
                }
            }
        }
    }
}

// Helper to output a signal to a JSON file
async fn output_signal(signal: &Signal, output_dir: &PathBuf) -> Result<()> {
    let timestamp = signal.timestamp.timestamp_millis();
    let filename = format!("{}_{}_{}_{}.json", 
        signal.symbol, 
        match signal.position_type {
            PositionType::Long => "LONG",
            PositionType::Short => "SHORT", 
        },
        timestamp,
        signal.id.split('-').next().unwrap_or("signal")
    );
    
    let path = output_dir.join(filename);
    
    // Convert signal to JSON
    let json = serde_json::to_string_pretty(signal)?;
    
    // Write to file
    let mut file = File::create(path)?;
    file.write_all(json.as_bytes())?;
    
    info!("Signal generated for {}: {:?} at ${:.2} (TP: ${:.2}, SL: ${:.2})",
        signal.symbol, signal.position_type, signal.price, signal.take_profit, signal.stop_loss);
        
    Ok(())
}