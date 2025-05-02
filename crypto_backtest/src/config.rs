// src/config.rs
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::error::Error;
use crate::strategy::StrategyConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    // Basic trading parameters
    pub initial_balance: f64,
    pub leverage: f64,
    pub max_risk_per_trade: f64,
    
    // Pivot and signal parameters
    pub pivot_lookback: usize,
    pub signal_lookback: usize,
    
    // Fibonacci parameters
    pub fib_threshold: f64,
    pub fib_initial: f64,
    pub fib_tp: f64,
    pub fib_sl: f64,
    pub fib_limit1: f64,
    pub fib_limit2: f64,
    
    // Strategy parameters
    pub min_signal_strength: f64,
    
    // Optimization parameters
    pub drop_threshold: f64,
    pub lookback_periods: Vec<usize>,
    pub initial_levels: Vec<f64>,
    pub tp_levels: Vec<f64>,
    pub sl_levels: Vec<f64>,
    pub limit1_levels: Vec<f64>,
    pub limit2_levels: Vec<f64>,
    pub threshold_factors: Vec<f64>,
    
    // Output and execution parameters
    pub data_dir: String,
    pub output_dir: String,
    pub parallel: bool,
    pub num_best_results: usize,
}

impl GlobalConfig {
    pub fn load_from_file(path: &str) -> Result<Self, Box<dyn Error>> {
        if Path::new(path).exists() {
            let content = fs::read_to_string(path)?;
            let config: GlobalConfig = serde_json::from_str(&content)?;
            Ok(config)
        } else {
            println!("Config file not found at {}, using defaults", path);
            Ok(GlobalConfig::default())
        }
    }
    
    /// Create a base StrategyConfig from this global config
    pub fn to_strategy_config(&self, name: String) -> StrategyConfig {
        StrategyConfig {
            name,
            initial_balance: self.initial_balance,
            leverage: self.leverage,
            max_risk_per_trade: self.max_risk_per_trade,
            pivot_lookback: self.pivot_lookback,
            signal_lookback: self.signal_lookback,
            fib_threshold: self.fib_threshold,
            fib_initial: self.fib_initial,
            fib_tp: self.fib_tp,
            fib_sl: self.fib_sl,
            fib_limit1: self.fib_limit1,
            fib_limit2: self.fib_limit2,
            min_signal_strength: self.min_signal_strength,
        }
    }
    
    /// Create a StrategyConfig for optimization with specific parameters
    pub fn to_strategy_config_with_params(
        &self,
        name: String,
        lookback: usize,
        initial: f64,
        tp: f64,
        sl: f64,
        limit1: f64,
        limit2: f64,
        threshold: f64,
    ) -> StrategyConfig {
        StrategyConfig {
            name,
            initial_balance: self.initial_balance,
            leverage: self.leverage,
            max_risk_per_trade: self.max_risk_per_trade,
            pivot_lookback: lookback,
            signal_lookback: self.signal_lookback,
            fib_threshold: threshold,
            fib_initial: initial,
            fib_tp: tp,
            fib_sl: sl,
            fib_limit1: limit1,
            fib_limit2: limit2,
            min_signal_strength: self.min_signal_strength,
        }
    }
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            initial_balance: 10000.0,
            leverage: 20.0,
            max_risk_per_trade: 0.02,
            pivot_lookback: 5,
            signal_lookback: 1,
            fib_threshold: 10.0,
            fib_initial: 0.382,
            fib_tp: 0.618,
            fib_sl: 0.236,
            fib_limit1: 0.5,
            fib_limit2: 0.786,
            min_signal_strength: 0.5,
            drop_threshold: 9000.0,
            lookback_periods: vec![5, 8, 10, 13],
            initial_levels: vec![0.236, 0.382, 0.5, 0.618, 0.786],
            tp_levels: vec![0.618, 1.0, 1.414, 1.618, 2.0, 2.618],
            sl_levels: vec![0.236, 0.382, 0.5, 0.618, 0.786],
            limit1_levels: vec![0.382, 0.5, 0.618, 0.786],
            limit2_levels: vec![0.786, 1.0, 1.272, 1.618],
            threshold_factors: vec![0.75, 1.0, 1.25, 1.5],
            data_dir: "data".to_string(),
            output_dir: "results".to_string(),
            parallel: true,
            num_best_results: 20,
        }
    }
}