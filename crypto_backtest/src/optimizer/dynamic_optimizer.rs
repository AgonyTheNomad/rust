use crate::models::{Candle, Trade};
use crate::backtest::{Backtester, BacktestMetrics};
use crate::strategy::{Strategy, StrategyConfig, AssetConfig};
use crate::indicators::PivotPoints;
use rayon::prelude::*;
use std::error::Error;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use serde::{Serialize, Deserialize};
use serde_json::json;

/// Configuration for the dynamic Fibonacci optimization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynamicOptimizationConfig {
    // Account settings
    pub initial_balance: f64,
    pub drop_threshold: f64,  // Stop optimization if balance falls below this
    
    // Pivot detection parameters
    pub lookback_periods: Vec<usize>,
    
    // Fibonacci parameters
    pub initial_levels: Vec<f64>,
    pub tp_levels: Vec<f64>,
    pub sl_levels: Vec<f64>,
    pub limit1_levels: Vec<f64>,
    pub limit2_levels: Vec<f64>,
    pub threshold_factors: Vec<f64>,
    
    // Output configuration
    pub output_dir: String,
    pub parallel: bool,
    pub num_best_results: usize,
}

impl Default for DynamicOptimizationConfig {
    fn default() -> Self {
        Self {
            initial_balance: 10_000.0,
            drop_threshold: 9_000.0,
            
            lookback_periods: vec![5, 8, 10, 13],
            
            initial_levels: vec![0.236, 0.382, 0.5, 0.618, 0.786],
            tp_levels: vec![0.618, 1.0, 1.414, 1.618, 2.0, 2.618],
            sl_levels: vec![0.236, 0.382, 0.5, 0.618, 0.786],
            limit1_levels: vec![0.382, 0.5, 0.618, 0.786],
            limit2_levels: vec![0.786, 1.0, 1.272, 1.618],
            threshold_factors: vec![0.75, 1.0, 1.25, 1.5],
            
            output_dir: "results".to_string(),
            parallel: true,
            num_best_results: 20,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationResult {
    pub lookback_period: usize,
    pub initial_level: f64,
    pub limit1_level: f64,
    pub limit2_level: f64,
    pub sl_level: f64,
    pub tp_level: f64,
    pub threshold_factor: f64,
    pub actual_threshold: f64,
    pub performance: HashMap<String, f64>,
}

/// The main dynamic Fibonacci optimizer
pub struct DynamicFibonacciOptimizer {
    config: DynamicOptimizationConfig,
}

impl DynamicFibonacciOptimizer {
    pub fn new(config: DynamicOptimizationConfig) -> Self {
        Self { config }
    }
    
    /// Create a new optimizer using configuration from a file
    pub fn from_file(file_path: &str) -> Result<Self, Box<dyn Error>> {
        let config = load_config_from_file(file_path)?;
        Ok(Self { config })
    }
    
    /// Get a clone of the current configuration
    pub fn get_config(&self) -> DynamicOptimizationConfig {
        self.config.clone()
    }
    
    /// Calculate a base threshold from candle data
    pub fn calculate_base_threshold(&self, candles: &[Candle], lookback: usize) -> f64 {
        // Use PivotPoints to identify pivots
        let mut pivot_detector = PivotPoints::new(lookback);
        let mut pivot_highs = Vec::new();
        let mut pivot_lows = Vec::new();
        
        // Identify all pivot points
        for candle in candles {
            let (pivot_high, pivot_low) = pivot_detector.identify_pivots(candle.high, candle.low);
            if let Some(high) = pivot_high {
                pivot_highs.push(high);
            }
            if let Some(low) = pivot_low {
                pivot_lows.push(low);
            }
        }
        
        // Calculate average range between highs and lows
        let mut ranges = Vec::new();
        for i in 0..pivot_highs.len().min(pivot_lows.len()) {
            if i > 0 {
                let range = (pivot_highs[i] - pivot_lows[i - 1]).abs();
                ranges.push(range);
            }
        }
        
        // Calculate average range
        if ranges.is_empty() {
            // Default threshold if we can't calculate
            return 10.0;
        }
        
        ranges.iter().sum::<f64>() / ranges.len() as f64
    }
    
    /// Optimize for a specific asset
    pub fn optimize_asset(&self, asset_name: &str, candles: &[Candle], leverage: f64, spread: f64) -> Result<Vec<OptimizationResult>, Box<dyn Error>> {
        println!("Starting optimization for {} with leverage {}", asset_name, leverage);
        
        // Create output directory
        let asset_output_dir = format!("{}/{}", self.config.output_dir, asset_name);
        std::fs::create_dir_all(&asset_output_dir)?;
        
        let param_combinations = self.generate_parameter_combinations(candles);
        let total_combinations = param_combinations.len();
        println!("Created {} parameter combinations to test", total_combinations);
        
        // Run optimizations
        let results = if self.config.parallel {
            self.run_parallel_optimizations(asset_name, candles, &param_combinations, leverage, spread)
        } else {
            self.run_sequential_optimizations(asset_name, candles, &param_combinations, leverage, spread)
        }?;
        
        // Sort results by total profit
        let mut sorted_results = results;
        sorted_results.sort_by(|a, b| {
            let a_profit = a.performance.get("Total Profit").unwrap_or(&0.0);
            let b_profit = b.performance.get("Total Profit").unwrap_or(&0.0);
            b_profit.partial_cmp(a_profit).unwrap_or(std::cmp::Ordering::Equal)
        });
        
        // Take the top N results
        let top_results: Vec<OptimizationResult> = sorted_results.into_iter()
            .take(self.config.num_best_results)
            .collect();
            
        // Save results to file
        self.save_results_to_file(asset_name, &top_results, &asset_output_dir)?;
        
        println!("Optimization complete for {}!", asset_name);
        println!("Top {} results saved to {}/{}_optimization_results.csv", 
                 top_results.len(), asset_output_dir, asset_name);
        
        Ok(top_results)
    }
    
    /// Generate all valid parameter combinations
    fn generate_parameter_combinations(&self, candles: &[Candle]) -> Vec<(usize, f64, f64, f64, f64, f64, f64, f64)> {
        let mut combinations = Vec::new();
        
        // Calculate base thresholds for each lookback period
        let mut base_thresholds = HashMap::new();
        for &lookback in &self.config.lookback_periods {
            let base_threshold = self.calculate_base_threshold(candles, lookback);
            base_thresholds.insert(lookback, base_threshold);
        }
        
        for &lookback in &self.config.lookback_periods {
            let base_threshold = *base_thresholds.get(&lookback).unwrap_or(&10.0);
            
            for &initial in &self.config.initial_levels {
                for &tp in &self.config.tp_levels {
                    for &sl in &self.config.sl_levels {
                        for &limit1 in &self.config.limit1_levels {
                            for &limit2 in &self.config.limit2_levels {
                                // Skip invalid combinations
                                if limit1 >= limit2 || sl >= limit1 {
                                    continue;
                                }
                                
                                for &threshold_factor in &self.config.threshold_factors {
                                    let actual_threshold = base_threshold * threshold_factor;
                                    combinations.push((lookback, initial, limit1, limit2, sl, tp, threshold_factor, actual_threshold));
                                }
                            }
                        }
                    }
                }
            }
        }
        
        combinations
    }
    
    /// Run optimizations in parallel
    fn run_parallel_optimizations(
        &self,
        asset_name: &str,
        candles: &[Candle],
        parameter_combinations: &[(usize, f64, f64, f64, f64, f64, f64, f64)],
        leverage: f64,
        spread: f64,
    ) -> Result<Vec<OptimizationResult>, Box<dyn Error>> {
        println!("Running parallel optimizations for {} using {} combinations...", 
                 asset_name, parameter_combinations.len());
        
        let drop_threshold = self.config.drop_threshold;
        let initial_balance = self.config.initial_balance;
        
        let results: Vec<OptimizationResult> = parameter_combinations
            .par_iter()
            .filter_map(|&(lookback, initial, limit1, limit2, sl, tp, threshold_factor, actual_threshold)| {
                // Create strategy configuration
                let config = StrategyConfig {
                    initial_balance,
                    leverage,
                    max_risk_per_trade: 0.02,
                    pivot_lookback: lookback,
                    signal_lookback: 1,
                    fib_threshold: actual_threshold,
                    fib_initial: initial,
                    fib_tp: tp,
                    fib_sl: sl,
                    fib_limit1: limit1,
                    fib_limit2: limit2,
                };
                
                let asset_config = AssetConfig {
                    name: asset_name.to_string(),
                    leverage,
                    spread,
                    avg_spread: spread * 2.0, // A simple heuristic 
                };
                
                let strategy = Strategy::new(config.clone(), asset_config);
                let mut backtester = Backtester::new(initial_balance, strategy);
                
                match backtester.run(candles) {
                    Ok(result) => {
                        let final_balance = initial_balance + result.metrics.total_profit;
                        if final_balance < drop_threshold {
                            return None;
                        }
                        
                        let mut performance = HashMap::new();
                        performance.insert("Total Trades".to_string(), result.metrics.total_trades as f64);
                        performance.insert("Win Rate".to_string(), result.metrics.win_rate);
                        performance.insert("Profit Factor".to_string(), result.metrics.profit_factor);
                        performance.insert("Total Profit".to_string(), result.metrics.total_profit);
                        performance.insert("Max Drawdown".to_string(), result.metrics.max_drawdown);
                        performance.insert("Sharpe Ratio".to_string(), result.metrics.sharpe_ratio);
                        performance.insert("Sortino Ratio".to_string(), result.metrics.sortino_ratio);
                        
                        if result.metrics.total_trades > 0 {
                            Some(OptimizationResult {
                                lookback_period: lookback,
                                initial_level: initial,
                                limit1_level: limit1,
                                limit2_level: limit2,
                                sl_level: sl,
                                tp_level: tp,
                                threshold_factor,
                                actual_threshold,
                                performance,
                            })
                        } else {
                            None
                        }
                    }
                    Err(_) => None,
                }
            })
            .collect();
            
        Ok(results)
    }
    
    /// Run optimizations sequentially—with progress reporting
    fn run_sequential_optimizations(
        &self,
        asset_name: &str,
        candles: &[Candle],
        parameter_combinations: &[(usize, f64, f64, f64, f64, f64, f64, f64)],
        leverage: f64,
        spread: f64,
    ) -> Result<Vec<OptimizationResult>, Box<dyn Error>> {
        println!("Running sequential optimizations for {} using {} combinations...", 
                 asset_name, parameter_combinations.len());
        
        let mut results = Vec::new();
        let total_combinations = parameter_combinations.len();
        let drop_threshold = self.config.drop_threshold;
        let initial_balance = self.config.initial_balance;
        
        for (i, &(lookback, initial, limit1, limit2, sl, tp, threshold_factor, actual_threshold)) in parameter_combinations.iter().enumerate() {
            // Report progress every 100 combinations
            if i % 100 == 0 {
                let progress = (i as f64 / total_combinations as f64) * 100.0;
                println!("Processing combination {} of {} ({:.2}% complete)", i + 1, total_combinations, progress);
            }
            
            let config = StrategyConfig {
                initial_balance,
                leverage,
                max_risk_per_trade: 0.02,
                pivot_lookback: lookback,
                signal_lookback: 1,
                fib_threshold: actual_threshold,
                fib_initial: initial,
                fib_tp: tp,
                fib_sl: sl,
                fib_limit1: limit1,
                fib_limit2: limit2,
            };
            
            let asset_config = AssetConfig {
                name: asset_name.to_string(),
                leverage,
                spread,
                avg_spread: spread * 2.0,
            };
            
            let strategy = Strategy::new(config.clone(), asset_config);
            let mut backtester = Backtester::new(initial_balance, strategy);
            
            match backtester.run(candles) {
                Ok(result) => {
                    let final_balance = initial_balance + result.metrics.total_profit;
                    if final_balance < drop_threshold {
                        continue;
                    }
                    
                    let mut performance = HashMap::new();
                    performance.insert("Total Trades".to_string(), result.metrics.total_trades as f64);
                    performance.insert("Win Rate".to_string(), result.metrics.win_rate);
                    performance.insert("Profit Factor".to_string(), result.metrics.profit_factor);
                    performance.insert("Total Profit".to_string(), result.metrics.total_profit);
                    performance.insert("Max Drawdown".to_string(), result.metrics.max_drawdown);
                    performance.insert("Sharpe Ratio".to_string(), result.metrics.sharpe_ratio);
                    performance.insert("Sortino Ratio".to_string(), result.metrics.sortino_ratio);
                    
                    if result.metrics.total_trades > 0 {
                        results.push(OptimizationResult {
                            lookback_period: lookback,
                            initial_level: initial,
                            limit1_level: limit1,
                            limit2_level: limit2,
                            sl_level: sl,
                            tp_level: tp,
                            threshold_factor,
                            actual_threshold,
                            performance,
                        });
                    }
                }
                Err(_) => continue,
            }
        }
        
        Ok(results)
    }
    
    /// Save the optimization results to CSV and JSON files
    fn save_results_to_file(
        &self,
        asset_name: &str,
        results: &[OptimizationResult],
        output_dir: &str,
    ) -> Result<(), Box<dyn Error>> {
        // Save to CSV
        let csv_path = format!("{}/{}_optimization_results.csv", output_dir, asset_name);
        let mut writer = csv::Writer::from_path(&csv_path)?;
        
        // Write header
        writer.write_record(&[
            "lookback_period",
            "initial_level",
            "limit1_level",
            "limit2_level",
            "sl_level",
            "tp_level",
            "threshold_factor",
            "actual_threshold",
            "Total Trades",
            "Win Rate",
            "Profit Factor",
            "Total Profit", 
            "Max Drawdown",
            "Sharpe Ratio",
            "Sortino Ratio",
        ])?;
        
        // Write data rows
        for result in results {
            writer.write_record(&[
                result.lookback_period.to_string(),
                format!("{:.3}", result.initial_level),
                format!("{:.3}", result.limit1_level),
                format!("{:.3}", result.limit2_level),
                format!("{:.3}", result.sl_level),
                format!("{:.3}", result.tp_level),
                format!("{:.2}", result.threshold_factor),
                format!("{:.2}", result.actual_threshold),
                result.performance.get("Total Trades").unwrap_or(&0.0).to_string(),
                format!("{:.4}", result.performance.get("Win Rate").unwrap_or(&0.0)),
                format!("{:.4}", result.performance.get("Profit Factor").unwrap_or(&0.0)),
                format!("{:.2}", result.performance.get("Total Profit").unwrap_or(&0.0)),
                format!("{:.4}", result.performance.get("Max Drawdown").unwrap_or(&0.0)),
                format!("{:.4}", result.performance.get("Sharpe Ratio").unwrap_or(&0.0)),
                format!("{:.4}", result.performance.get("Sortino Ratio").unwrap_or(&0.0)),
            ])?;
        }
        
        writer.flush()?;
        
        // Save to JSON
        let json_path = format!("{}/{}_optimization_results.json", output_dir, asset_name);
        let mut json_file = File::create(&json_path)?;
        
        let output = json!({
            "asset": asset_name,
            "total_combinations_tested": results.len(),
            "optimization_config": {
                "initial_balance": self.config.initial_balance,
                "drop_threshold": self.config.drop_threshold,
                "lookback_periods": self.config.lookback_periods,
                "initial_levels": self.config.initial_levels,
                "tp_levels": self.config.tp_levels,
                "sl_levels": self.config.sl_levels,
                "limit1_levels": self.config.limit1_levels,
                "limit2_levels": self.config.limit2_levels,
                "threshold_factors": self.config.threshold_factors,
            },
            "top_results": results,
        });
        
        write!(json_file, "{}", serde_json::to_string_pretty(&output)?)?;
        
        Ok(())
    }
    
    /// Run a final backtest using the best parameters
    pub fn run_final_backtest(
        &self,
        asset_name: &str,
        candles: &[Candle],
        best_result: &OptimizationResult,
        leverage: f64,
        spread: f64,
    ) -> Result<(), Box<dyn Error>> {
        println!("Running final backtest for {} with best parameters...", asset_name);
        
        let asset_output_dir = format!("{}/{}", self.config.output_dir, asset_name);
        
        let config = StrategyConfig {
            initial_balance: self.config.initial_balance,
            leverage,
            max_risk_per_trade: 0.02,
            pivot_lookback: best_result.lookback_period,
            signal_lookback: 1,
            fib_threshold: best_result.actual_threshold,
            fib_initial: best_result.initial_level,
            fib_tp: best_result.tp_level,
            fib_sl: best_result.sl_level,
            fib_limit1: best_result.limit1_level,
            fib_limit2: best_result.limit2_level,
        };
        
        let asset_config = AssetConfig {
            name: asset_name.to_string(),
            leverage,
            spread,
            avg_spread: spread * 2.0,
        };
        
        let strategy = Strategy::new(config.clone(), asset_config);
        let mut backtester = Backtester::new(self.config.initial_balance, strategy);
        
        let results = backtester.run(candles)?;
        
        // Save trades to CSV if there are any
        if !results.trades.is_empty() {
            let trade_file = format!("{}/{}_trades.csv", asset_output_dir, asset_name);
            let mut writer = csv::Writer::from_path(trade_file)?;
            
            writer.write_record(&[
                "Entry Time", "Exit Time", "Type", "Entry Price", "Exit Price", 
                "Size", "P&L", "Risk %", "Profit Factor", "Margin Used"
            ])?;
            
            for trade in &results.trades {
                writer.write_record(&[
                    trade.entry_time.clone(),
                    trade.exit_time.clone(),
                    trade.position_type.clone(),
                    format!("{:.2}", trade.entry_price),
                    format!("{:.2}", trade.exit_price),
                    format!("{:.6}", trade.size),
                    format!("{:.2}", trade.pnl),
                    format!("{:.2}%", trade.risk_percent * 100.0),
                    format!("{:.4}", trade.profit_factor),
                    format!("{:.2}", trade.margin_used),
                ])?;
            }
            
            writer.flush()?;
        }
        
        // Calculate winning and losing trades
        let winning_trades = (results.metrics.win_rate * results.metrics.total_trades as f64).round() as usize;
        let losing_trades = results.metrics.total_trades - winning_trades;
        
        // Save metrics to JSON
        let metrics_file = format!("{}/{}_metrics.json", asset_output_dir, asset_name);
        let mut metrics_file = File::create(metrics_file)?;
        
        let metrics_json = json!({
            "strategy_config": {
                "initial_balance": config.initial_balance,
                "leverage": config.leverage,
                "max_risk_per_trade": config.max_risk_per_trade,
                "pivot_lookback": config.pivot_lookback,
                "signal_lookback": config.signal_lookback,
                "fib_threshold": config.fib_threshold,
                "fib_initial": config.fib_initial,
                "fib_tp": config.fib_tp,
                "fib_sl": config.fib_sl,
                "fib_limit1": config.fib_limit1,
                "fib_limit2": config.fib_limit2,
            },
            "performance": {
                "total_trades": results.metrics.total_trades,
                "winning_trades": winning_trades,
                "losing_trades": losing_trades,
                "win_rate": results.metrics.win_rate,
                "profit_factor": results.metrics.profit_factor,
                "total_profit": results.metrics.total_profit,
                "max_drawdown": results.metrics.max_drawdown,
                "sharpe_ratio": results.metrics.sharpe_ratio,
                "sortino_ratio": results.metrics.sortino_ratio,
                "risk_reward_ratio": results.metrics.risk_reward_ratio,
                "final_balance": self.config.initial_balance + results.metrics.total_profit,
            },
            "execution_info": {
                "duration_ms": results.duration.as_millis(),
            }
        });
        
        write!(metrics_file, "{}", serde_json::to_string_pretty(&metrics_json)?)?;
        
        println!("Final backtest completed for {}!", asset_name);
        println!("Results saved to {}", asset_output_dir);
        
        Ok(())
    }
}

/// Function to load assets from a JSON configuration file and optimize each one
pub fn optimize_assets_from_config(
    assets_file: &str,
    optimization_config: DynamicOptimizationConfig,
) -> Result<(), Box<dyn Error>> {
    // Read assets configuration
    let file_content = std::fs::read_to_string(assets_file)?;
    let json_data: serde_json::Value = serde_json::from_str(&file_content)?;
    
    // Extract the assets array
    let assets = json_data["assets"].as_array()
        .ok_or_else(|| format!("No 'assets' array found in {}", assets_file))?;
    
    let optimizer = DynamicFibonacciOptimizer::new(optimization_config.clone());
    
    // Create output directory
    std::fs::create_dir_all(&optimizer.config.output_dir)?;
    
    println!("Found {} assets to optimize", assets.len());
    
    for asset_json in assets {
        let asset_name = asset_json["name"].as_str()
            .ok_or_else(|| format!("Asset missing 'name' field in {}", assets_file))?;
        
        let leverage = asset_json["leverage"].as_f64()
            .ok_or_else(|| format!("Asset '{}' missing 'leverage' field", asset_name))?;
        
        let spread = asset_json["spread"].as_f64()
            .ok_or_else(|| format!("Asset '{}' missing 'spread' field", asset_name))?;
        
        println!("\nOptimizing {} (leverage: {}, spread: {})", asset_name, leverage, spread);
        
        // Try to load candle data
        let candle_path = format!("data/{}.csv", asset_name);
        
        if !Path::new(&candle_path).exists() {
            println!("Warning: Candle data not found for {}, skipping", asset_name);
            continue;
        }
        
        match crate::fetch_data::load_candles_from_csv(&candle_path) {
            Ok(mut candles) => {
                // Filter out invalid candles
                candles.retain(|c| c.volume > 0.0);
                
                if candles.is_empty() {
                    println!("Warning: No valid candles for {}, skipping", asset_name);
                    continue;
                }
                
                println!("Loaded {} valid candles for {}", candles.len(), asset_name);
                
                // Run optimization
                match optimizer.optimize_asset(asset_name, &candles, leverage, spread) {
                    Ok(results) => {
                        if !results.is_empty() {
                            println!("Running final backtest with best parameters for {}", asset_name);
                            let best_result = &results[0];
                            if let Err(e) = optimizer.run_final_backtest(asset_name, &candles, best_result, leverage, spread) {
                                println!("Error running final backtest for {}: {}", asset_name, e);
                            }
                        } else {
                            println!("No valid optimization results found for {}", asset_name);
                        }
                    },
                    Err(e) => println!("Error optimizing {}: {}", asset_name, e),
                }
            },
            Err(e) => println!("Error loading candles for {}: {}", asset_name, e),
        }
    }
    
    println!("\nOptimization of all assets complete!");
    Ok(())
}

/// Helper function to create a python-like default optimization config
pub fn python_like_optimization_config() -> DynamicOptimizationConfig {
    // First try to load from the optimization_config.json file
    if let Ok(config) = load_config_from_file("optimization_config.json") {
        return config;
    }
    
    // Fall back to hardcoded defaults if file loading fails
    DynamicOptimizationConfig {
        initial_balance: 10_000.0,
        drop_threshold: 9_000.0,
        
        lookback_periods: vec![5, 8, 10, 13],
        
        initial_levels: vec![0.236, 0.382, 0.5, 0.618, 0.786],
        tp_levels: vec![0.618, 1.0, 1.414, 1.618, 2.0, 2.618],
        sl_levels: vec![0.236, 0.382, 0.5, 0.618],
        limit1_levels: vec![0.382, 0.5, 0.618, 0.786],
        limit2_levels: vec![0.786, 1.0, 1.272, 1.618],
        threshold_factors: vec![0.75, 1.0, 1.25, 1.5],
        
        output_dir: "results".to_string(),
        parallel: true,
        num_best_results: 20,
    }
}

/// Load optimization configuration from a JSON file
pub fn load_config_from_file(file_path: &str) -> Result<DynamicOptimizationConfig, Box<dyn Error>> {
    println!("Loading optimization configuration from {}", file_path);
    
    // Read the file content
    let file_content = std::fs::read_to_string(file_path)?;
    
    // Parse JSON
    let json_config: serde_json::Value = serde_json::from_str(&file_content)?;
    
    // Extract configuration values with proper error handling
    let config = DynamicOptimizationConfig {
        initial_balance: json_config["initial_balance"].as_f64()
            .ok_or_else(|| format!("Missing or invalid 'initial_balance' in {}", file_path))?,
            
        drop_threshold: json_config["drop_threshold"].as_f64()
            .ok_or_else(|| format!("Missing or invalid 'drop_threshold' in {}", file_path))?,
            
        lookback_periods: json_config["lookback_periods"].as_array()
            .ok_or_else(|| format!("Missing 'lookback_periods' array in {}", file_path))?
            .iter()
            .filter_map(|v| v.as_u64().map(|i| i as usize))
            .collect(),
            
        initial_levels: json_config["initial_levels"].as_array()
            .ok_or_else(|| format!("Missing 'initial_levels' array in {}", file_path))?
            .iter()
            .filter_map(|v| v.as_f64())
            .collect(),
            
        tp_levels: json_config["tp_levels"].as_array()
            .ok_or_else(|| format!("Missing 'tp_levels' array in {}", file_path))?
            .iter()
            .filter_map(|v| v.as_f64())
            .collect(),
            
        sl_levels: json_config["sl_levels"].as_array()
            .ok_or_else(|| format!("Missing 'sl_levels' array in {}", file_path))?
            .iter()
            .filter_map(|v| v.as_f64())
            .collect(),
            
        limit1_levels: json_config["limit1_levels"].as_array()
            .ok_or_else(|| format!("Missing 'limit1_levels' array in {}", file_path))?
            .iter()
            .filter_map(|v| v.as_f64())
            .collect(),
            
        limit2_levels: json_config["limit2_levels"].as_array()
            .ok_or_else(|| format!("Missing 'limit2_levels' array in {}", file_path))?
            .iter()
            .filter_map(|v| v.as_f64())
            .collect(),
            
        threshold_factors: json_config["threshold_factors"].as_array()
            .ok_or_else(|| format!("Missing 'threshold_factors' array in {}", file_path))?
            .iter()
            .filter_map(|v| v.as_f64())
            .collect(),
            
        output_dir: json_config["output_dir"].as_str()
            .unwrap_or("results")
            .to_string(),
            
        parallel: json_config["parallel"].as_bool()
            .unwrap_or(true),
            
        num_best_results: json_config["num_best_results"].as_u64()
            .unwrap_or(20) as usize,
    };
    
    // Validate the configuration
    if config.lookback_periods.is_empty() {
        return Err("No valid lookback periods provided".into());
    }
    if config.initial_levels.is_empty() {
        return Err("No valid initial levels provided".into());
    }
    if config.tp_levels.is_empty() {
        return Err("No valid take profit levels provided".into());
    }
    if config.sl_levels.is_empty() {
        return Err("No valid stop loss levels provided".into());
    }
    if config.limit1_levels.is_empty() {
        return Err("No valid limit1 levels provided".into());
    }
    if config.limit2_levels.is_empty() {
        return Err("No valid limit2 levels provided".into());
    }
    if config.threshold_factors.is_empty() {
        return Err("No valid threshold factors provided".into());
    }
    
    println!("Successfully loaded optimization configuration:");
    println!("  Lookback periods: {:?}", config.lookback_periods);
    println!("  Initial levels: {:?}", config.initial_levels);
    println!("  Take profit levels: {:?}", config.tp_levels);
    println!("  Stop loss levels: {:?}", config.sl_levels);
    println!("  Limit1 levels: {:?}", config.limit1_levels);
    println!("  Limit2 levels: {:?}", config.limit2_levels);
    println!("  Threshold factors: {:?}", config.threshold_factors);
    println!("  Output directory: {}", config.output_dir);
    
    Ok(config)
}