// src/bin/signal_generator.rs
use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use clap::Parser;
use log::*;
use rust_trader::{
    influxdb::{InfluxDBClient, InfluxDBConfig},
    models::{Account, Candle, Signal, Position, PositionType},
    setup_logging,
    SignalFileManager, 
    strategy::{Strategy, StrategyConfig, AssetConfig},
    risk::{RiskManager, RiskParameters, PositionCalculator},
    exchange::AccountReader,
};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, BufReader};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration as StdDuration;
use tokio::time;
use serde::Deserialize;

// Define version constants
const VERSION: &str = env!("CARGO_PKG_VERSION");
const GENERATOR_NAME: &str = "fibonacci_pivot";
const ACCOUNT_FILE_PATH: &str = "./account_info.json";
const ACCOUNT_MAX_AGE_SECONDS: u64 = 300; // 5 minutes

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
    
    /// Max age for signals in seconds (signals older than this won't be processed)
    #[clap(long, default_value = "120")]
    max_signal_age: i64,
    
    /// Minimum signal strength (0.0-1.0)
    #[clap(long, default_value = "0.7")]
    min_signal_strength: f64,
    
    /// Signal cooldown period in minutes
    #[clap(long, default_value = "5")]
    signal_cooldown: i64,
    
    /// Path to backtest results directory for optimized parameters
    #[clap(long, default_value = "../crypto_backtest/results")]
    backtest_dir: PathBuf,
    
    /// Path to account info file
    #[clap(long, default_value = "./account_info.json")]
    account_file: PathBuf,
}

// Configuration structure
#[derive(serde::Deserialize)]
struct Config {
    general: GeneralConfig,
    influxdb: InfluxDBConfig,
    strategy: StrategyConfig,
    assets: HashMap<String, AssetConfig>,
    risk: RiskParameters,
}

#[derive(serde::Deserialize)]
struct GeneralConfig {
    refresh_interval: u64,  // seconds
    data_dir: PathBuf,
    log_level: String,
    max_candles: usize,
    historical_days: u32,
}

// Backtest result structure for parsing optimized parameters
#[derive(Deserialize)]
struct BacktestResult {
    strategy_config: StrategyConfig,
    performance: BacktestPerformance,
}

#[derive(Deserialize)]
struct BacktestPerformance {
    win_rate: f64,
    profit_factor: Option<f64>,
    total_trades: usize,
}

// Load configuration from TOML file
fn load_config<P: AsRef<std::path::Path>>(path: P) -> Result<Config> {
    let mut file = File::open(path).context("Failed to open config file")?;
    let mut contents = String::new();
    std::io::Read::read_to_string(&mut file, &mut contents).context("Failed to read config file")?;
    let config: Config = toml::from_str(&contents).context("Failed to parse config file")?;
    Ok(config)
}

// Load backtest configuration from JSON file
fn load_backtest_config<P: AsRef<Path>>(path: P) -> Result<StrategyConfig> {
    let file = File::open(&path)
        .context(format!("Failed to open backtest file: {:?}", path.as_ref()))?;
    let reader = BufReader::new(file);
    let backtest: BacktestResult = serde_json::from_reader(reader)
        .context(format!("Failed to parse backtest JSON from {:?}", path.as_ref()))?;
    
    Ok(backtest.strategy_config)
}

// Helper to track signal generator statistics
#[derive(Debug, Clone, Default)]
struct SignalStats {
    start_time: DateTime<Utc>,
    signals_generated: usize,
    signals_by_symbol: HashMap<String, usize>,
    signals_by_type: HashMap<String, usize>,
    candles_processed: usize,
    last_signal_time: HashMap<String, DateTime<Utc>>,
    errors: usize,
}

impl SignalStats {
    fn new() -> Self {
        Self {
            start_time: Utc::now(),
            signals_generated: 0,
            signals_by_symbol: HashMap::new(),
            signals_by_type: HashMap::new(),
            candles_processed: 0,
            last_signal_time: HashMap::new(),
            errors: 0,
        }
    }
    
    fn record_signal(&mut self, signal: &Signal) {
        self.signals_generated += 1;
        
        // Track by symbol
        *self.signals_by_symbol.entry(signal.symbol.clone()).or_insert(0) += 1;
        
        // Track by type
        let type_str = format!("{:?}", signal.position_type);
        *self.signals_by_type.entry(type_str).or_insert(0) += 1;
        
        // Track last time
        self.last_signal_time.insert(signal.symbol.clone(), signal.timestamp);
    }
    
    fn record_candle(&mut self) {
        self.candles_processed += 1;
    }
    
    fn record_error(&mut self) {
        self.errors += 1;
    }
    
    fn runtime(&self) -> Duration {
        Utc::now() - self.start_time
    }
    
    fn print_stats(&self) {
        info!("Signal Generator Stats:");
        info!("  Running for: {:?}", self.runtime());
        info!("  Total signals: {}", self.signals_generated);
        info!("  Candles processed: {}", self.candles_processed);
        info!("  Errors encountered: {}", self.errors);
        info!("  Signals by symbol:");
        for (symbol, count) in &self.signals_by_symbol {
            info!("    {}: {}", symbol, count);
        }
        info!("  Signals by type:");
        for (type_name, count) in &self.signals_by_type {
            info!("    {}: {}", type_name, count);
        }
    }
    
    fn write_to_file(&self, path: &Path) -> Result<()> {
        let stats_json = serde_json::json!({
            "start_time": self.start_time,
            "current_time": Utc::now(),
            "runtime_seconds": self.runtime().num_seconds(),
            "signals_generated": self.signals_generated,
            "candles_processed": self.candles_processed,
            "errors": self.errors,
            "signals_by_symbol": self.signals_by_symbol,
            "signals_by_type": self.signals_by_type,
        });
        
        let json_str = serde_json::to_string_pretty(&stats_json)?;
        fs::write(path, json_str)?;
        
        Ok(())
    }
}

// Create PID file to prevent multiple instances
fn create_pid_file() -> Result<()> {
    let pid = std::process::id();
    let pid_path = Path::new("signal_generator.pid");
    
    if pid_path.exists() {
        // Read current PID file
        let pid_str = fs::read_to_string(&pid_path)?;
        
        if let Ok(_old_pid) = pid_str.trim().parse::<u32>() {
            // Check if process still running (Unix-specific)
            #[cfg(unix)]
            {
                use std::process::Command;
                let output = Command::new("ps")
                    .arg("-p")
                    .arg(old_pid.to_string())
                    .output()?;
                
                if output.status.success() && !output.stdout.is_empty() {
                    return Err(anyhow::anyhow!(
                        "Another instance is already running (PID: {})", old_pid));
                }
            }
            
            // If we get here on Unix, or always on Windows, assume stale PID file
            warn!("Removing stale PID file from previous instance");
        }
        
        // Remove stale PID file
        fs::remove_file(&pid_path)?;
    }
    
    // Write current PID
    fs::write(pid_path, pid.to_string())?;
    info!("Created PID file with PID {}", pid);
    
    Ok(())
}

// Validate candle data
fn is_valid_candle(candle: &Candle) -> bool {
    // Check for zero or negative values
    if candle.high <= 0.0 || candle.low <= 0.0 || 
       candle.open <= 0.0 || candle.close <= 0.0 {
        return false;
    }
    
    // Check for unrealistic values
    if candle.high < candle.low {
        return false;
    }
    
    // Check for abnormal values (price jumps)
    if candle.high > candle.low * 1.5 {
        // 50% price jump in a single candle is suspicious
        return false;
    }
    
    true
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line args
    let args = Args::parse();
    
    // Create PID file
    create_pid_file()?;
    
    // Set up logging
    setup_logging();
    
    // Setup clean shutdown signal handling
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    
    // Spawn signal handler (Unix only)
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        
        tokio::spawn(async move {
            let mut sigterm = signal(SignalKind::terminate())
                .expect("failed to install SIGTERM handler");
                
            let mut sigint = signal(SignalKind::interrupt())
                .expect("failed to install SIGINT handler");
                
            tokio::select! {
                _ = sigterm.recv() => {
                    info!("Received SIGTERM signal");
                }
                _ = sigint.recv() => {
                    info!("Received SIGINT signal");
                }
            }
            
            info!("Shutting down gracefully...");
            r.store(false, Ordering::SeqCst);
        });
    }
    
    // Initialize statistics tracker
    let mut stats = SignalStats::new();
    
    // Load configuration
    let config = load_config(&args.config)?;
    
    // Create directories if they don't exist
    std::fs::create_dir_all(&args.output)?;
    std::fs::create_dir_all(&args.archive)?;
    std::fs::create_dir_all(&args.commands)?;
    
    // Initialize signal file manager
    let signal_manager = SignalFileManager::new(&args.output.to_string_lossy(), VERSION);
    
    // Clean up any stale locks
    match signal_manager.clean_stale_locks(10) {
        Ok(count) => {
            if count > 0 {
                info!("Cleaned up {} stale lock files", count);
            }
        },
        Err(e) => {
            error!("Error cleaning stale locks: {}", e);
            stats.record_error();
        }
    }
    
    // Connect to InfluxDB
    let influx_client = InfluxDBClient::new(config.influxdb.clone())?;
    
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
    
    // Read account balance from file - fail if not found
    let account_reader = AccountReader::new(
        args.account_file.to_str().unwrap_or(ACCOUNT_FILE_PATH),
        ACCOUNT_MAX_AGE_SECONDS
    );
    
    let account_info = account_reader.read_account_info()
        .context("Failed to read account information")?;
    
    let account_balance = account_info.balance;
    info!("Using account balance: ${:.2}", account_balance);
    
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
        
        // Modify strategy config with command-line parameters
        let mut strategy_config = config.strategy.clone();
        strategy_config.min_signal_strength = args.min_signal_strength;
        
        // Check multiple possible file paths for this symbol's backtest results
        // Try various path formats to handle different directory/file structures
        let possible_paths = [
            // Format 1: ../crypto_backtest/results/BTC/BTC_metrics.json (uppercase directory)
            args.backtest_dir.join(symbol).join(format!("{}_metrics.json", symbol)),
            
            // Format 2: ../crypto_backtest/results/BTC_metrics.json
            args.backtest_dir.join(format!("{}_metrics.json", symbol)),
        ];
        
        // Debug output to see which paths we're checking
        for path in &possible_paths {
            debug!("Checking for backtest results at: {:?} - exists: {}", path, path.exists());
        }
        
        // Try each path until we find a valid one
        let mut found_config = false;
        for backtest_path in &possible_paths {
            if backtest_path.exists() {
                info!("Found backtest config at {:?}", backtest_path);
                match load_backtest_config(&backtest_path) {
                    Ok(backtest_config) => {
                        // Process configuration
                        info!("Loaded optimized parameters for {}: pivot_lookback={}, signal_lookback={}, fib_threshold={:.4}, initial={:.4}, tp={:.4}, sl={:.4}, limit1={:.4}, limit2={:.4}",
                             symbol, 
                             backtest_config.pivot_lookback,
                             backtest_config.signal_lookback,
                             backtest_config.fib_threshold,
                             backtest_config.fib_initial,
                             backtest_config.fib_tp,
                             backtest_config.fib_sl,
                             backtest_config.fib_limit1,
                             backtest_config.fib_limit2);
                         
                        // Update strategy config with optimized values
                        strategy_config.pivot_lookback = backtest_config.pivot_lookback;
                        strategy_config.signal_lookback = backtest_config.signal_lookback;
                        strategy_config.fib_threshold = backtest_config.fib_threshold;
                        strategy_config.fib_initial = backtest_config.fib_initial;
                        strategy_config.fib_tp = backtest_config.fib_tp;
                        strategy_config.fib_sl = backtest_config.fib_sl;
                        strategy_config.fib_limit1 = backtest_config.fib_limit1;
                        strategy_config.fib_limit2 = backtest_config.fib_limit2;
                        found_config = true;
                        break;
                    },
                    Err(e) => {
                        warn!("Could not load backtest file {:?}: {}", backtest_path, e);
                    }
                }
            }
        }
        
        if !found_config {
            debug!("No backtest configuration found for {}", symbol);
        }
        
        let mut strategy = Strategy::new(strategy_config, asset_config);
        
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
    
    // Last time we printed and wrote stats
    let mut last_stats_time = Utc::now();
    
    // Create a mock account for risk calculations with the real balance
    let mock_account = Account {
        balance: account_balance,
        equity: account_balance,
        used_margin: 0.0,
        positions: HashMap::new(),
    };
    
    // Create risk manager with the real account balance
    let risk_manager = RiskManager::new(config.risk.clone(), account_balance)
    .context("Failed to create risk manager")?;
    
    // Main loop - run continuously
    let mut interval = time::interval(StdDuration::from_secs(1));
    
    while running.load(Ordering::SeqCst) {
        interval.tick().await;
        
        // Check for command files
        match signal_manager.check_commands(&args.commands.to_string_lossy()) {
            Ok(commands) => {
                for command in commands {
                    info!("Processing command: {}", command);
                    // Handle shutdown command
                    if command == "shutdown.cmd" || command == "stop.cmd" {
                        info!("Received stop command. Shutting down...");
                        running.store(false, Ordering::SeqCst);
                        break;
                    }
                }
            },
            Err(e) => {
                error!("Error checking commands: {}", e);
                stats.record_error();
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
                stats.record_error();
            }
        }
        
        // Get current time
        let now = Utc::now();
        
        // Process each symbol
        for (symbol, (strategy, last_update)) in &mut symbol_states {
            // Check if this symbol has a trading lock
            if signal_manager.has_trading_lock(symbol) {
                debug!("Skipping {} - trading lock exists", symbol);
                continue;
            }
            
            // Get new candles since last update
            let new_candles = match influx_client.get_candles(symbol, last_update, &now).await {
                Ok(candles) => candles,
                Err(e) => {
                    error!("Error getting candles for {}: {}", symbol, e);
                    stats.record_error();
                    continue;
                }
            };
            
            if !new_candles.is_empty() {
                debug!("Got {} new candles for {}", new_candles.len(), symbol);
                
                // Sort candles by time (oldest first) to properly update state
                let mut sorted_candles = new_candles.clone();
                sorted_candles.sort_by(|a, b| {
                    let time_a = DateTime::parse_from_rfc3339(&a.time).unwrap_or_default();
                    let time_b = DateTime::parse_from_rfc3339(&b.time).unwrap_or_default();
                    time_a.cmp(&time_b)
                });
                
                // First update strategy state with all candles
                for candle in &sorted_candles {
                    if !is_valid_candle(candle) {
                        warn!("Skipping invalid candle for {}: {:?}", symbol, candle);
                        continue;
                    }
                    
                    stats.record_candle();
                }
                
                // Only process the most recent candle for signal generation
                if let Some(latest_candle) = sorted_candles.last() {
                    if !is_valid_candle(latest_candle) {
                        warn!("Latest candle for {} is invalid, skipping signal generation", symbol);
                    } else {
                        debug!("Processing latest candle for {}: {}", symbol, latest_candle.time);
                        
                        // Generate signals for the latest candle
                        match strategy.analyze_candle(latest_candle) {
                            Ok(mut signal_positions) => {
                                // Process any generated signals
                                for (signal, position) in &mut signal_positions {
                                    // Try to create a lock for this symbol
                                    match signal_manager.check_and_create_lock(symbol) {
                                        Ok(true) => {
                                            // IMPORTANT: Store the original signal before any modifications
                                            let original_signal = signal.clone();
                                            
                                            // Calculate position sizing using risk management
                                            if let (Some(limit1), Some(limit2)) = (position.limit1_price, position.limit2_price) {
                                                // IMPORTANT: Store the original scaling ratios before modifying sizes
                                                let limit1_ratio = position.limit1_size;
                                                let limit2_ratio = position.limit2_size;
                                                
                                                match risk_manager.calculate_scaled_position(
                                                        &mock_account,
                                                        position.entry_price,
                                                        position.stop_loss,
                                                        position.take_profit,
                                                        limit1,
                                                        limit2,
                                                        position.position_type.clone()
                                                    ) {
                                                    Ok(scaling) => {
                                                        // Update position with calculated values
                                                        position.size = scaling.initial_position_size;
                                                        
                                                        // FIXED: Set the limit sizes using scaling positions
                                                        position.limit1_size = scaling.limit1_position_size;
                                                        position.limit2_size = scaling.limit2_position_size;
                                                        
                                                        // If for some reason these end up as 0.0, 
                                                        // use the original ratios as a fallback
                                                        if position.limit1_size <= 0.0 {
                                                            position.limit1_size = position.size * limit1_ratio;
                                                        }
                                                        
                                                        if position.limit2_size <= 0.0 {
                                                            position.limit2_size = position.size * limit2_ratio;
                                                        }
                                                        
                                                        position.new_tp1 = Some(scaling.new_tp1);
                                                        position.new_tp2 = Some(scaling.new_tp2);
                                                        position.risk_percent = scaling.final_risk;
                                                        
                                                        info!("Calculated position sizing: base={:.6}, limit1={:.6}, limit2={:.6}, risk={:.2}%",
                                                              position.size, position.limit1_size, position.limit2_size, 
                                                              position.risk_percent * 100.0);
                                                    },
                                                    Err(e) => {
                                                        error!("Failed to calculate position sizing: {}", e);
                                                        stats.record_error();
                                                        
                                                        // Fall back to simple position sizing
                                                        let stop_distance = (position.entry_price - position.stop_loss).abs();
                                                        let risk_amount = mock_account.balance * config.risk.max_risk_per_trade;
                                                        let base_size = (risk_amount / stop_distance).min(config.risk.max_position_size);
                                                        
                                                        position.size = base_size;
                                                        
                                                        // FIXED: Always calculate limit sizes based on base size and ratios
                                                        position.limit1_size = base_size * limit1_ratio;
                                                        position.limit2_size = base_size * limit2_ratio;
                                                        
                                                        position.risk_percent = config.risk.max_risk_per_trade;
                                                        
                                                        info!("Using fallback position sizing: base={:.6}, limit1={:.6}, limit2={:.6}, risk={:.2}%",
                                                              position.size, position.limit1_size, position.limit2_size, 
                                                              position.risk_percent * 100.0);
                                                    }
                                                }
                                            } else {
                                                // If limit prices aren't set, use simple sizing
                                                let stop_distance = (position.entry_price - position.stop_loss).abs();
                                                let risk_amount = mock_account.balance * config.risk.max_risk_per_trade;
                                                position.size = (risk_amount / stop_distance).min(config.risk.max_position_size);
                                                position.risk_percent = config.risk.max_risk_per_trade;
                                                
                                                // Make sure limit sizes are properly set even for simple sizing
                                                if position.limit1_size <= 0.0 && position.size > 0.0 {
                                                    position.limit1_size = position.size * 3.0; // Default 3:1 ratio
                                                }
                                                
                                                if position.limit2_size <= 0.0 && position.size > 0.0 {
                                                    position.limit2_size = position.size * 5.0; // Default 5:1 ratio
                                                }
                                                
                                                info!("Using simple position sizing: size={:.6}, risk={:.2}%",
                                                      position.size, position.risk_percent * 100.0);
                                            }
                                            
                                            // Write signal with age check - using original signal to preserve it
                                            match signal_manager.write_signal(&original_signal, Some(position), Some(args.max_signal_age)) {
                                                Ok(_) => {
                                                    total_signals += 1;
                                                    stats.record_signal(&original_signal);
                                                    
                                                    // Log signal to InfluxDB
                                                    if let Err(e) = influx_client.write_signal(&original_signal).await {
                                                        error!("Error writing signal to InfluxDB: {}", e);
                                                        stats.record_error();
                                                    }
                                                    
                                                    info!("Generated {} signal for {} at {}: Entry={}, TP={}, SL={}, Size={:.6}, Strength={}",
                                                        format!("{:?}", original_signal.position_type),
                                                        original_signal.symbol,
                                                        original_signal.timestamp.format("%H:%M:%S"),
                                                        original_signal.price,
                                                        original_signal.take_profit,
                                                        original_signal.stop_loss,
                                                        position.size,
                                                        original_signal.strength);
                                                },
                                                Err(e) => {
                                                    error!("Error writing signal file: {}", e);
                                                    stats.record_error();
                                                }
                                            }
                                            
                                            // Release the lock after processing
                                            if let Err(e) = signal_manager.release_lock(symbol) {
                                                error!("Error releasing trading lock for {}: {}", symbol, e);
                                                stats.record_error();
                                            }
                                        },
                                        Ok(false) => {
                                            debug!("Failed to acquire trading lock for {}, skipping signal", symbol);
                                        },
                                        Err(e) => {
                                            error!("Error checking trading lock for {}: {}", symbol, e);
                                            stats.record_error();
                                        }
                                    }
                                }
                            },
                            Err(e) => {
                                error!("Error analyzing candle for {}: {}", symbol, e);
                                stats.record_error();
                            }
                        }
                    }
                }
                
                // Update last processed time
                if let Some(latest_candle) = sorted_candles.last() {
                    match DateTime::parse_from_rfc3339(&latest_candle.time) {
                        Ok(time) => {
                            *last_update = time.with_timezone(&Utc);
                        },
                        Err(e) => {
                            error!("Failed to parse candle time: {}", e);
                            stats.record_error();
                        }
                    }
                }
            }
        }
        
        // Periodically check if account balance has changed
        if (now - last_stats_time).num_seconds() > 300 {
            // Try to refresh account information
            match account_reader.read_account_info() {
                Ok(new_account_info) => {
                    // Check if balance has changed significantly
                    if (new_account_info.balance - account_balance).abs() > 0.01 {
                        info!("Account balance updated: ${:.2} -> ${:.2}", 
                             account_balance, new_account_info.balance);
                             
                        // Update mock account with new balance
                        // Note: Since we're storing this in a local variable and the RiskManager 
                        // is using a copy of the value, we can't update them without recreating them
                        // In a real implementation, you might want to make these shareable with Arcs and Mutexes
                        
                        // For now, log the change but don't update (would require code restructuring)
                        // In a future version, consider making these updatable during runtime
                    }
                },
                Err(e) => {
                    // Just log the error, don't stop execution
                    warn!("Failed to refresh account info: {}", e);
                }
            }
            
            // Print and save stats (every 5 minutes)
            stats.print_stats();
            if let Err(e) = stats.write_to_file(Path::new("signal_generator_stats.json")) {
                error!("Error writing stats to file: {}", e);
            }
            last_stats_time = now;
        }
        
        // Log a heartbeat message periodically
        if total_signals > 0 && total_signals % 10 == 0 {
            info!("Signal generator heartbeat: {} total signals generated", total_signals);
        }
    }
    
    // Final cleanup before exit
    info!("Cleaning up before exit...");
    
    // Print final statistics
    stats.print_stats();
    if let Err(e) = stats.write_to_file(Path::new("signal_generator_stats_final.json")) {
        error!("Error writing final stats to file: {}", e);
    }
    
    // Remove PID file
    if let Err(e) = fs::remove_file("signal_generator.pid") {
        error!("Error removing PID file: {}", e);
    }
    
    info!("Signal generator shut down successfully");
    
    Ok(())
}