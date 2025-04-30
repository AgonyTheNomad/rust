// src/backtest/mod.rs
mod config_loader;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::collections::HashMap;
use log::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestPerformance {
    pub final_balance: f64,
    pub losing_trades: usize,
    pub max_drawdown: f64,
    pub profit_factor: f64,
    pub risk_reward_ratio: f64,
    pub sharpe_ratio: f64,
    pub sortino_ratio: f64,
    pub total_profit: f64,
    pub total_trades: usize,
    pub win_rate: f64,
    pub winning_trades: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyConfig {
    pub fib_initial: f64,
    pub fib_limit1: f64,
    pub fib_limit2: f64,
    pub fib_sl: f64,
    pub fib_threshold: f64,
    pub fib_tp: f64,
    pub initial_balance: f64,
    pub leverage: f64,
    pub max_risk_per_trade: f64,
    pub pivot_lookback: usize,
    pub signal_lookback: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestResult {
    pub execution_info: serde_json::Value,
    pub performance: BacktestPerformance,
    pub strategy_config: StrategyConfig,
}

// New struct to track performance by symbol
#[derive(Debug, Clone)]
pub struct SymbolPerformance {
    pub symbol: String,
    pub win_rate: f64,
    pub profit_factor: f64,
    pub total_trades: usize,
}

pub fn load_backtest_config<P: AsRef<Path>>(path: P) -> Result<StrategyConfig> {
    let file = File::open(path).context("Failed to open backtest result file")?;
    let reader = BufReader::new(file);
    let backtest: BacktestResult = serde_json::from_reader(reader)
        .context("Failed to parse backtest result JSON")?;
    
    Ok(backtest.strategy_config)
}

// Load the best backtest result from a directory
pub fn load_best_backtest<P: AsRef<Path>>(dir: P, metric: &str) -> Result<StrategyConfig> {
    use std::fs;
    
    let mut best_value = f64::NEG_INFINITY;
    let mut best_config = None;
    
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
            if let Ok(result) = load_backtest_result(&path) {
                let value = match metric {
                    "profit_factor" => result.performance.profit_factor,
                    "sharpe_ratio" => result.performance.sharpe_ratio,
                    "final_balance" => result.performance.final_balance,
                    "win_rate" => result.performance.win_rate,
                    _ => result.performance.profit_factor, // Default to profit factor
                };
                
                if value > best_value {
                    best_value = value;
                    best_config = Some(result.strategy_config);
                }
            }
        }
    }
    
    best_config.context("No valid backtest results found")
}

fn load_backtest_result<P: AsRef<Path>>(path: P) -> Result<BacktestResult> {
    let file = File::open(path).context("Failed to open backtest result file")?;
    let reader = BufReader::new(file);
    let result: BacktestResult = serde_json::from_reader(reader)
        .context("Failed to parse backtest result JSON")?;
    
    Ok(result)
}

// New function to get performance metrics by symbol
pub fn get_symbol_performance<P: AsRef<Path>>(dir: P) -> Result<HashMap<String, SymbolPerformance>> {
    use std::fs;
    
    let mut performance_map = HashMap::new();
    
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
            // Extract symbol from filename (assuming format like "BTC_optimization_results.json")
            let file_name = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            
            let symbol = file_name.split('_').next().unwrap_or("").to_uppercase();
            if symbol.is_empty() {
                continue;
            }
            
            if let Ok(result) = load_backtest_result(&path) {
                performance_map.insert(symbol.clone(), SymbolPerformance {
                    symbol,
                    win_rate: result.performance.win_rate,
                    profit_factor: result.performance.profit_factor,
                    total_trades: result.performance.total_trades,
                });
            }
        }
    }
    
    Ok(performance_map)
}

// Function to filter symbols based on win rate threshold
pub fn filter_symbols(performance_map: &HashMap<String, SymbolPerformance>, 
                     min_win_rate: f64,
                     symbols: &[String]) -> Vec<String> {
    symbols.iter()
        .filter(|symbol| {
            if let Some(perf) = performance_map.get(*symbol) {
                if perf.win_rate >= min_win_rate {
                    return true;
                }
                info!("Filtering out {} due to insufficient win rate: {:.2}% (threshold {:.2}%)", 
                      symbol, perf.win_rate * 100.0, min_win_rate * 100.0);
                return false;
            }
            
            info!("No backtest data for {}, including by default", symbol);
            true // Include symbols without backtest data
        })
        .cloned()
        .collect()
}