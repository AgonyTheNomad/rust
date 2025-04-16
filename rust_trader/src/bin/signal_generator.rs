// src/bin/signal_generator.rs
use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use clap::Parser;
use log::*;
use rust_trader::{
    influxdb::{InfluxDBClient, InfluxDBConfig},
    models::{Candle, Signal, PositionType},
    setup_logging,
    signals::{fibonacci::FibonacciLevels, pivots::PivotPoints, file_manager::SignalFileManager},
    strategy::{Strategy, StrategyConfig, AssetConfig},
};
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration as StdDuration;
use tokio::time;

// Define version constants
const VERSION: &str = env!("CARGO_PKG_VERSION");
const GENERATOR_NAME: &str = "fibonacci_pivot";

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
    
    /// Archive directory for old signals
    #[clap(long, default_value = "signals/archive")]
    archive: PathBuf,
    
    /// Command directory for IPC
    #[clap(long, default_value = "commands")]
    commands: PathBuf,
    
    /// Interval in seconds between updates
    #[clap(short, long, default_value = "60")]
    interval: u64,
    
    /// Max age in hours for archiving signals
    #[clap(long, default_value = "24")]
    max_age: i64,
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
    
    // Create directories if they don't exist
    std::fs::create_dir_all(&args.output)?;
    std::fs::create_dir_all(&args.archive)?;
    std::fs::create_dir_all(&args.commands)?;
    
    // Initialize signal file manager
    let signal_manager = SignalFileManager::new(&args.output.to_string_lossy(), VERSION);
    
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
    
    info!("Signal Generator v{} started. Analyzing {} symbols: {}", 
          VERSION, symbols.len(), symbols.join(", "));
    
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
    
    // Variable to track total signals generated
    let mut total_signals = 0;
    
    // Main loop - run continuously
    let mut interval = time::interval(StdDuration::from_secs(args.interval));
    
    loop {
        interval.tick().await;
        
        // Check for command files
        match signal_manager.check_commands(&args.commands.to_string_lossy()) {
            Ok(commands) => {
                for command in commands {
                    info!("Processing command: {}", command);
                    // TODO: Handle commands (stop, pause, etc.)
                }
            },
            Err(e) => {
                error!("Error checking commands: {}", e);
            }
        }
        
        // Archive old signals
        match signal_manager.archive_old_signals(&args.archive.to_string_lossy(), args.max_age) {
            Ok(count) => {
                if count > 0 {
                    debug!("Archived {} old signal files", count);
                }
            },
            Err(e) => {
                error!("Error archiving old signals: {}", e);
            }
        }
        
        // Process each symbol
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
                            match signal_manager.write_signal(signal) {
                                Ok(_) => {
                                    total_signals += 1;
                                },
                                Err(e) => {
                                    error!("Error writing signal file: {}", e);
                                }
                            }
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
        
        // Log a heartbeat message periodically
        if total_signals > 0 && total_signals % 10 == 0 {
            info!("Signal generator heartbeat: {} total signals generated", total_signals);
        }
    }
}