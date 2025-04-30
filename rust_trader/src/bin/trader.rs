// src/bin/trader.rs
use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use clap::{Parser, Subcommand};
use log::*;
use rust_trader::{
    exchange::{Exchange, ExchangeConfig, create_exchange},
    influxdb::{InfluxDBClient, InfluxDBConfig},
    models::{Candle, Position, Signal, PositionStatus},
    risk::{RiskManager, RiskParameters},
    setup_logging,
    strategy::{Strategy, StrategyConfig, AssetConfig},
    backtest::{load_best_backtest, get_symbol_performance, filter_symbols},
};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration as StdDuration};
use tokio::sync::Mutex;
use tokio::time;
use toml;

// CLI Arguments using clap
#[derive(Parser)]
#[clap(author, version, about)]
struct Args {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run in trading mode
    Trade {
        /// Path to configuration file
        #[clap(short, long, default_value = "config/trader.toml")]
        config: PathBuf,
        
        /// Symbols to trade (comma separated)
        #[clap(short, long)]
        symbols: Option<String>,
        
        /// Run in dry-run mode (no real trades)
        #[clap(long)]
        dry_run: bool,
        
        /// Path to backtest results directory for optimized parameters
        #[clap(long)]
        backtest_dir: Option<PathBuf>,
        
        /// Minimum win rate threshold for trading (0.0-1.0)
        #[clap(long, default_value = "0.5")]
        min_win_rate: f64,
    },
    
    /// Fetch historical data and save to CSV
    Fetch {
        /// Symbol to fetch
        #[clap(short, long)]
        symbol: String,
        
        /// Number of days to fetch
        #[clap(short, long, default_value = "30")]
        days: u32,
        
        /// Output directory
        #[clap(short, long, default_value = "data")]
        output: PathBuf,
    },
    
    /// Monitor open positions
    Monitor {
        /// Path to configuration file
        #[clap(short, long, default_value = "config/trader.toml")]
        config: PathBuf,
    },
}

// Configuration
#[derive(serde::Deserialize)]
struct Config {
    general: GeneralConfig,
    exchange: ExchangeConfig,
    influxdb: InfluxDBConfig,
    risk: RiskParameters,
    strategy: StrategyConfig,
    assets: HashMap<String, AssetConfig>,
}

#[derive(serde::Deserialize, Clone)]
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
    file.read_to_string(&mut contents).context("Failed to read config file")?;
    let config: Config = toml::from_str(&contents).context("Failed to parse config file")?;
    Ok(config)
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line args
    let args = Args::parse();
    
    // Set up logging
    setup_logging();
    
    match args.command {
        Commands::Trade { config, symbols, dry_run, backtest_dir, min_win_rate } => {
            trade(config, symbols, dry_run, backtest_dir, min_win_rate).await?;
        },
        Commands::Fetch { symbol, days, output } => {
            fetch_historical_data(&symbol, days, &output).await?;
        },
        Commands::Monitor { config } => {
            monitor_positions(config).await?;
        },
    }
    
    Ok(())
}

// Main trading function
async fn trade(
    config_path: PathBuf, 
    symbol_list: Option<String>, 
    dry_run: bool, 
    backtest_dir: Option<PathBuf>,
    min_win_rate: f64
) -> Result<()> {
    // Load configuration
    let mut config = load_config(config_path)?;
    
    // Get list of symbols to analyze
    let mut symbols = match symbol_list {
        Some(list) => list.split(',').map(|s| s.trim().to_uppercase()).collect::<Vec<_>>(),
        None => {
            // Try to get symbols from InfluxDB
            let influx_client = InfluxDBClient::new(config.influxdb.clone())?;
            match influx_client.get_symbols().await {
                Ok(symbols) => {
                    if symbols.is_empty() {
                        warn!("No symbols found in InfluxDB, using default list");
                        vec!["BTC".to_string(), "ETH".to_string()]
                    } else {
                        symbols
                    }
                },
                Err(e) => {
                    warn!("Failed to get symbols from InfluxDB: {}", e);
                    vec!["BTC".to_string(), "ETH".to_string()]
                }
            }
        }
    };
    
    info!("Starting trading system in {} mode", if dry_run { "dry-run" } else { "live" });
    
    // Load optimized parameters and filter symbols by win rate if backtest directory is provided
    if let Some(dir) = backtest_dir {
        info!("Loading optimized parameters from backtest results");
        
        // Load optimized strategy parameters
        match load_best_backtest(&dir, "profit_factor") {
            Ok(optimized_config) => {
                info!("Found optimal configuration");
                
                // Update the strategy config with optimized parameters
                config.strategy.fib_threshold = optimized_config.fib_threshold;
                config.strategy.fib_initial = optimized_config.fib_initial;
                config.strategy.fib_tp = optimized_config.fib_tp;
                config.strategy.fib_sl = optimized_config.fib_sl;
                config.strategy.fib_limit1 = optimized_config.fib_limit1;
                config.strategy.fib_limit2 = optimized_config.fib_limit2;
                config.strategy.pivot_lookback = optimized_config.pivot_lookback;
                config.strategy.signal_lookback = optimized_config.signal_lookback;
                
                // Update risk parameters if included in backtest
                config.risk.max_risk_per_trade = optimized_config.max_risk_per_trade;
                
                info!("Strategy updated with optimized parameters: pivot_lookback={}, signal_lookback={}, fib_threshold={:.4}, fib_initial={:.4}, fib_tp={:.4}, fib_sl={:.4}",
                      config.strategy.pivot_lookback,
                      config.strategy.signal_lookback,
                      config.strategy.fib_threshold,
                      config.strategy.fib_initial,
                      config.strategy.fib_tp,
                      config.strategy.fib_sl);
            },
            Err(e) => {
                warn!("Could not load optimized parameters: {}", e);
                info!("Proceeding with default configuration");
            }
        }
        
        // Filter symbols by win rate
        match get_symbol_performance(&dir) {
            Ok(performance_map) => {
                info!("Found performance data for {} symbols", performance_map.len());
                
                // Filter symbols with win rate >= min_win_rate
                let original_count = symbols.len();
                symbols = filter_symbols(&performance_map, min_win_rate, &symbols);
                
                info!("Filtered symbols from {} to {} based on win rate >= {:.1}%", 
                     original_count, symbols.len(), min_win_rate * 100.0);
                
                // Log the included symbols with their performance
                for symbol in &symbols {
                    if let Some(perf) = performance_map.get(symbol) {
                        info!("{}: Win Rate = {:.2}%, Profit Factor = {:.2}, Trades = {}", 
                             symbol, perf.win_rate * 100.0, perf.profit_factor, perf.total_trades);
                    }
                }
            },
            Err(e) => {
                warn!("Could not load symbol performance data: {}", e);
                info!("Proceeding with all symbols without filtering");
            }
        }
    }
    
    if symbols.is_empty() {
        return Err(anyhow::anyhow!("No symbols to trade after filtering"));
    }
    
    info!("Trading {} symbols: {}", symbols.len(), symbols.join(", "));
    
    // Connect to InfluxDB
    let influx_client = InfluxDBClient::new(config.influxdb)?;
    
    // Create exchange client
    let exchange = if dry_run {
        info!("Using mock exchange for dry-run mode");
        create_exchange(config.exchange)?
    } else {
        info!("Connecting to {} exchange", config.exchange.name);
        create_exchange(config.exchange)?
    };
    
    // Initialize risk manager
    let account_balance = exchange.get_balance().await.map_err(|e| anyhow::anyhow!("Failed to get account balance: {}", e))?;
    let risk_manager = Arc::new(Mutex::new(RiskManager::new(config.risk, account_balance)));
    
    // Create trading state
    let trading_state = Arc::new(Mutex::new(TradingState {
        positions: HashMap::new(),
        signals: Vec::new(),
        last_update: HashMap::new(),
        account_balance: account_balance,
    }));
    
    // Process each symbol in its own task
    let mut handles = Vec::new();
    
    for symbol in symbols {
        // Clone our shared state and create a new Arc for the exchange
        let exchange_ref = Arc::new(exchange.clone_box());
        let influx_clone = Arc::new(influx_client.clone());
        let risk_clone = Arc::clone(&risk_manager);
        let state_clone = Arc::clone(&trading_state);
        let config_clone = config.general.clone();
        let symbol_cloned = symbol.clone();
        
        // Look up asset config or use default
        let asset_config = match config.assets.get(&symbol) {
            Some(asset) => asset.clone(),
            None => {
                warn!("No asset config found for {}, using default values", symbol);
                AssetConfig {
                    name: symbol.clone(),
                    leverage: 20.0,
                    spread: 0.0005,
                    avg_spread: 0.001,
                }
            }
        };
        
        // Create strategy
        let strategy = Strategy::new(config.strategy.clone(), asset_config);
        
        // Start processing task
        let handle = tokio::spawn(async move {
            let result = process_symbol(
                exchange_ref,
                influx_clone,
                risk_clone,
                state_clone,
                strategy,
                &symbol_cloned,
                config_clone,
                dry_run,
            ).await;
            
            if let Err(e) = result {
                error!("Error processing symbol {}: {}", symbol_cloned, e);
            }
        });
        
        handles.push(handle);
    }
    
    // Wait for all tasks to complete (which they shouldn't unless there's an error)
    for handle in handles {
        if let Err(e) = handle.await {
            error!("Task error: {}", e);
        }
    }
    
    Ok(())
}

// Trading state shared between symbol processing tasks
struct TradingState {
    positions: HashMap<String, Position>,  // Key is position ID
    signals: Vec<Signal>,
    last_update: HashMap<String, DateTime<Utc>>,
    account_balance: f64,
}

impl TradingState {
    // Check if a symbol has an open position
    pub fn has_open_position_for_symbol(&self, symbol: &str) -> bool {
        self.positions.values().any(|pos| 
            pos.symbol == symbol && pos.status == PositionStatus::Open
        )
    }
    
    // Add a new position
    pub fn add_position(&mut self, position: Position) {
        self.positions.insert(position.id.clone(), position);
    }
    
    // Update an existing position
    pub fn update_position(&mut self, position: Position) {
        if let Some(existing) = self.positions.get_mut(&position.id) {
            *existing = position;
        }
    }
    
    // Close a position by ID
    pub fn close_position(&mut self, position_id: &str) {
        if let Some(position) = self.positions.get_mut(position_id) {
            position.mark_as_closed();
        }
    }
    
    // Remove a closed position by ID
    pub fn remove_position(&mut self, position_id: &str) {
        self.positions.remove(position_id);
    }
}

// Process a single symbol
async fn process_symbol(
    exchange: Arc<Box<dyn Exchange>>,
    influx_client: Arc<InfluxDBClient>,
    risk_manager: Arc<Mutex<RiskManager>>,
    trading_state: Arc<Mutex<TradingState>>,
    mut strategy: Strategy,
    symbol: &str,
    config: GeneralConfig,
    dry_run: bool,
) -> Result<()> {
    info!("Starting processing for {}", symbol);
    
    // Initial data load
    let now = Utc::now();
    let start_time = now - Duration::days(config.historical_days as i64);
    
    info!("Loading historical data for {} from {} to {}", 
        symbol, start_time.to_rfc3339(), now.to_rfc3339());
    
    let mut candles = influx_client.get_candles(symbol, &start_time, &now).await?;
    
    if candles.is_empty() {
        return Err(anyhow::anyhow!("No candles found for {}", symbol));
    }
    
    info!("Loaded {} candles for {}", candles.len(), symbol);
    
    // Run strategy on historical data to establish state
    info!("Running strategy on historical data for {}", symbol);
    strategy.initialize_with_history(&candles)?;
    
    // Get last processed time
    let mut last_candle_time = match candles.last() {
        Some(candle) => DateTime::parse_from_rfc3339(&candle.time)
            .map_err(|e| anyhow::anyhow!("Failed to parse candle time: {}", e))?
            .with_timezone(&Utc),
        None => {
            return Err(anyhow::anyhow!("No candles found for {}", symbol));
        },
    };
    
    // Store last update time
    {
        let mut state = trading_state.lock().await;
        state.last_update.insert(symbol.to_string(), last_candle_time);
    }
    
    // Set up interval for regular processing
    let mut interval = time::interval(StdDuration::from_secs(config.refresh_interval));
    
    // Main processing loop
    loop {
        interval.tick().await;
        
        // Get new candles since last update
        let now = Utc::now();
        let new_candles = influx_client.get_candles(symbol, &last_candle_time, &now).await?;
        
        if !new_candles.is_empty() {
            debug!("Got {} new candles for {}", new_candles.len(), symbol);
            
            // Update last candle time
            if let Some(last_candle) = new_candles.last() {
                last_candle_time = DateTime::parse_from_rfc3339(&last_candle.time)
                    .map_err(|e| anyhow::anyhow!("Failed to parse candle time: {}", e))?
                    .with_timezone(&Utc);
                
                // Update state
                {
                    let mut state = trading_state.lock().await;
                    state.last_update.insert(symbol.to_string(), last_candle_time);
                }
            }
            
            // Process each new candle
            for candle in &new_candles {
                process_candle(
                    exchange.as_ref(),
                    influx_client.clone(),
                    Arc::clone(&risk_manager),
                    Arc::clone(&trading_state),
                    &mut strategy,
                    symbol,
                    candle,
                    dry_run
                ).await?;
            }
            
            // Append to our candle history
            candles.extend_from_slice(&new_candles);
            
            // Trim history if needed
            if candles.len() > config.max_candles {
                candles = candles.split_off(candles.len() - config.max_candles);
            }
        }
        
        // Check for open positions that might need updating
        update_positions(
            exchange.as_ref(),
            Arc::clone(&trading_state),
            symbol,
            dry_run
        ).await?;
    }
}

// Process a single candle
async fn process_candle(
    exchange: &Box<dyn Exchange>,
    influx_client: Arc<InfluxDBClient>,
    risk_manager: Arc<Mutex<RiskManager>>,
    trading_state: Arc<Mutex<TradingState>>,
    strategy: &mut Strategy,
    symbol: &str,
    candle: &Candle,
    dry_run: bool,
) -> Result<()> {
    // Check if we already have an open position for this symbol
    let has_open_position = {
        let state = trading_state.lock().await;
        state.has_open_position_for_symbol(symbol)
    };
    
    // Skip signal generation if we already have an open position for this symbol
    if has_open_position {
        debug!("Already have an open position for {}. Skipping signal generation.", symbol);
        return Ok(());
    }
    
    // Analyze candle with strategy
    let signals = strategy.analyze_candle(candle)?;
    
    if !signals.is_empty() {
        info!("Generated {} signals for {} from candle {}", 
            signals.len(), symbol, candle.time);
        
        // Process each signal
        for signal in signals {
            // Store signal
            {
                let mut state = trading_state.lock().await;
                state.signals.push(signal.clone());
            }
            
            // Log to InfluxDB directly using influx_client
            if let Err(e) = influx_client.write_signal(&signal).await {
                warn!("Failed to log signal to InfluxDB: {}", e);
            }
            
            if !dry_run {
                // Check if we can take this trade
                let can_trade = {
                    let rm = risk_manager.lock().await;
                    let _state = trading_state.lock().await;
                    
                    let account = exchange.get_account_info().await
                        .map_err(|e| anyhow::anyhow!("Failed to get account info: {}", e))?;
                    
                    rm.can_open_new_position(&account)
                };
                
                if can_trade {
                    // Calculate position size
                    let position_info = {
                        let rm = risk_manager.lock().await;
                        
                        let account = exchange.get_account_info().await
                            .map_err(|e| anyhow::anyhow!("Failed to get account info: {}", e))?;
                            
                        rm.calculate_position_size(
                            &account,
                            signal.price,
                            signal.stop_loss,
                            signal.position_type.clone()
                        )?
                    };
                    
                    // Create position object
                    let position = create_position(
                        symbol,
                        signal,
                        position_info.size,
                        strategy.get_asset_config().leverage,
                    );
                    
                    // Open the position on the exchange
                    info!("Opening {:?} position for {} at {}: size = {}, SL = {}, TP = {}", 
                        position.position_type, symbol, position.entry_price,
                        position.size, position.stop_loss, position.take_profit);
                    
                    match exchange.open_position(&position).await {
                        Ok(updated_position) => {
                            info!("Position opened successfully: {}", updated_position.id);
                            
                            // Store position in state
                            let mut state = trading_state.lock().await;
                            state.add_position(updated_position);
                        },
                        Err(e) => {
                            error!("Failed to open position: {}", e);
                        }
                    }
                } else {
                    info!("Skipping trade due to risk management constraints");
                }
            } else {
                info!("[DRY RUN] Would open {:?} position for {} at {} (SL: {}, TP: {})",
                    signal.position_type, symbol, signal.price, signal.stop_loss, signal.take_profit);
            }
        }
    }
    
    Ok(())
}

// Create a position from a signal
fn create_position(
    symbol: &str,
    signal: Signal,
    size: f64,
    leverage: f64,
) -> Position {
    let risk_percent = 0.02; // This would come from risk management
    
    Position {
        id: uuid::Uuid::new_v4().to_string(),
        symbol: symbol.to_string(),
        entry_time: Utc::now(),
        entry_price: signal.price,
        size,
        stop_loss: signal.stop_loss,
        take_profit: signal.take_profit,
        position_type: signal.position_type,
        risk_percent,
        margin_used: (size * signal.price) / leverage,
        status: PositionStatus::Pending,
        limit1_price: None, // Would be set based on scaling strategy
        limit2_price: None,
        limit1_hit: false,
        limit2_hit: false,
        limit1_size: 0.0,
        limit2_size: 0.0,
        new_tp1: None,
        new_tp2: None,
        entry_order_id: None,
        tp_order_id: None,
        sl_order_id: None,
        limit1_order_id: None,
        limit2_order_id: None,
    }
}

// Update existing positions
async fn update_positions(
    exchange: &Box<dyn Exchange>,
    trading_state: Arc<Mutex<TradingState>>,
    symbol: &str,
    dry_run: bool,
) -> Result<()> {
    if dry_run {
        return Ok(());
    }
    
    // Get current price
    let current_price = exchange.get_ticker(symbol).await
        .map_err(|e| anyhow::anyhow!("Failed to get ticker: {}", e))?;
    
    // Get all positions from the exchange
    let exchange_positions = exchange.get_positions().await
        .map_err(|e| anyhow::anyhow!("Failed to get positions from exchange: {}", e))?;
    
    // Create a set of position IDs still open on the exchange
    let exchange_position_ids: std::collections::HashSet<String> = exchange_positions
        .iter()
        .map(|p| p.id.clone())
        .collect();
    
    // Get positions for this symbol from our state
    let positions_to_update = {
        let state = trading_state.lock().await;
        state.positions.iter()
            .filter(|(_, pos)| pos.symbol == symbol)
            .map(|(_, pos)| pos.clone())
            .collect::<Vec<_>>()
    };
    
    // Check for closed positions and update our state
    for position in positions_to_update {
        // Check if this position still exists on the exchange
        let position_closed = !exchange_position_ids.contains(&position.id);
        
        if position_closed {
            info!("Position {} for {} has been closed", position.id, symbol);
            
            // Remove from our state
            let mut state = trading_state.lock().await;
            state.remove_position(&position.id);
            
            // Now we can process new signals for this symbol
        }
        
        // Check if any limit orders have been hit
        if position.is_hit_limit1(current_price) && !position.limit1_hit {
            info!("Limit 1 hit for position {} at {}", position.id, current_price);
            
            if !dry_run {
                // Update position
                let mut updated = position.clone();
                updated.limit1_hit = true;
                updated.size += updated.limit1_size;
                
                if let Some(new_tp) = updated.new_tp1 {
                    updated.take_profit = new_tp;
                }
                
                // Update on exchange
                match exchange.update_position(&updated).await {
                    Ok(_) => {
                        info!("Position updated for limit1 hit");
                        
                        // Update in state
                        let mut state = trading_state.lock().await;
                        state.update_position(updated);
                    },
                    Err(e) => {
                        error!("Failed to update position for limit1 hit: {}", e);
                    }
                }
            }
        }
        
        if position.is_hit_limit2(current_price) && position.limit1_hit && !position.limit2_hit {
            info!("Limit 2 hit for position {} at {}", position.id, current_price);
            
            if !dry_run {
                // Update position
                let mut updated = position.clone();
                updated.limit2_hit = true;
                updated.size += updated.limit2_size;
                
                if let Some(new_tp) = updated.new_tp2 {
                    updated.take_profit = new_tp;
                }
                
                // Update on exchange
                match exchange.update_position(&updated).await {
                    Ok(_) => {
                        info!("Position updated for limit2 hit");
                        
                        // Update in state
                        let mut state = trading_state.lock().await;
                        state.update_position(updated);
                    },
                    Err(e) => {
                        error!("Failed to update position for limit2 hit: {}", e);
                    }
                }
            }
        }
    }
    
    Ok(())
}

// Fetch historical data and save to CSV
async fn fetch_historical_data(symbol: &str, days: u32, output_dir: &PathBuf) -> Result<()> {
    // Load InfluxDB config from environment
    let influxdb_config = InfluxDBConfig::from_env()?;
    let influx_client = InfluxDBClient::new(influxdb_config)?;
    
    let now = Utc::now();
    let start_time = now - Duration::days(days as i64);
    
    info!("Fetching historical data for {} from {} to {}", 
        symbol, start_time.to_rfc3339(), now.to_rfc3339());
    
    let candles = influx_client.get_candles(symbol, &start_time, &now).await?;
    
    if candles.is_empty() {
        return Err(anyhow::anyhow!("No candles found for {}", symbol));
    }
    
    info!("Got {} candles for {}", candles.len(), symbol);
    
    // Create output directory if it doesn't exist
    std::fs::create_dir_all(output_dir)?;
    
    // Save to CSV
    let file_path = output_dir.join(format!("{}.csv", symbol.to_lowercase()));
    let mut writer = csv::Writer::from_path(file_path.clone())?;
    
    // Write header
    writer.write_record(&[
        "Timestamp", "Open", "High", "Low", "Close", "Volume", "NumTrades"
    ])?;
    
    // Write data
    for candle in &candles {
        writer.write_record(&[
            candle.time.clone(),
            candle.open.to_string(),
            candle.high.to_string(),
            candle.low.to_string(),
            candle.close.to_string(),
            candle.volume.to_string(),
            candle.num_trades.to_string(),
        ])?;
    }
    
    writer.flush()?;
    
    info!("Saved {} candles to {}", candles.len(), file_path.display());
    
    Ok(())
}

// Monitor open positions
async fn monitor_positions(config_path: PathBuf) -> Result<()> {
    // Load configuration
    let config = load_config(config_path)?;
    
    // Connect to exchange
    let exchange = create_exchange(config.exchange)?;
    
    let refresh_interval = config.general.refresh_interval;
    
    info!("Starting position monitor with refresh interval of {} seconds", refresh_interval);
    
    // Set up interval for regular checks
    let mut interval = time::interval(StdDuration::from_secs(refresh_interval));
    
    // Main monitoring loop
    loop {
        interval.tick().await;
        
        // Get current positions
        match exchange.get_positions().await {
            Ok(positions) => {
                if positions.is_empty() {
                    info!("No open positions");
                } else {
                    info!("Open positions: {}", positions.len());
                    
                    for position in positions {
                        // Get current price
                        let current_price = match exchange.get_ticker(&position.symbol).await {
                            Ok(price) => price,
                            Err(e) => {
                                error!("Failed to get price for {}: {}", position.symbol, e);
                                continue;
                            }
                        };
                        
                        let pnl = position.current_pnl(current_price);
                        let pnl_percent = pnl / (position.size * position.entry_price) * 100.0;
                        
                        let status = if pnl > 0.0 {
                            format!("\x1b[32m+${:.2} (+{:.2}%)\x1b[0m", pnl, pnl_percent)
                        } else {
                            format!("\x1b[31m-${:.2} ({:.2}%)\x1b[0m", pnl.abs(), pnl_percent)
                        };
                        
                        info!("{} {:?} position: Entry=${:.2}, Current=${:.2}, Size={:.6}, PnL={}",
                            position.symbol, position.position_type, position.entry_price, 
                            current_price, position.size, status);
                    }
                }
            },
            Err(e) => {
                error!("Failed to get positions: {}", e);
            }
        }
    }
}