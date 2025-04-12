// src/optimizer/mod.rs
use std::fs;
use serde_json;

// First declare all modules
pub mod dynamic_optimizer;  // This needs to be declared before it's used
pub mod grid_optimizer;

pub mod fibonacci_optimizer {
    // Re-export everything from dynamic_optimizer under the fibonacci_optimizer namespace.
    pub use super::dynamic_optimizer::*;
    
    // For backward compatibility, we alias some types.
    pub type FibonacciOptimizer = super::dynamic_optimizer::DynamicFibonacciOptimizer;
    pub type FibonacciOptimizationConfig = super::dynamic_optimizer::DynamicOptimizationConfig;
    pub type OptimizationMetric = &'static str;  // A simple type for compatibility.
    pub type OptimizationResult = super::dynamic_optimizer::OptimizationResult;
    
    // Modified to allow loading from JSON file
    pub fn default_fibonacci_optimization_config() -> FibonacciOptimizationConfig {
        // Try to read from file first
        if let Ok(config_str) = std::fs::read_to_string("optimization_config.json") {
            if let Ok(json_config) = serde_json::from_str::<serde_json::Value>(&config_str) {
                return FibonacciOptimizationConfig {
                    initial_balance: json_config["initial_balance"].as_f64().unwrap_or(10000.0),
                    drop_threshold: json_config["drop_threshold"].as_f64().unwrap_or(9000.0),
                    
                    lookback_periods: json_config["lookback_periods"]
                        .as_array()
                        .map(|arr| arr.iter().filter_map(|v| v.as_u64().map(|i| i as usize)).collect())
                        .unwrap_or_else(|| vec![5, 8, 10, 13]),
                        
                    initial_levels: json_config["initial_levels"]
                        .as_array()
                        .map(|arr| arr.iter().filter_map(|v| v.as_f64()).collect())
                        .unwrap_or_else(|| vec![0.236, 0.382, 0.5, 0.618, 0.786]),
                        
                    tp_levels: json_config["tp_levels"]
                        .as_array()
                        .map(|arr| arr.iter().filter_map(|v| v.as_f64()).collect())
                        .unwrap_or_else(|| vec![0.618, 1.0, 1.414, 1.618, 2.0, 2.618]),
                        
                    sl_levels: json_config["sl_levels"]
                        .as_array()
                        .map(|arr| arr.iter().filter_map(|v| v.as_f64()).collect())
                        .unwrap_or_else(|| vec![1.0, 1.618, 2.0, 2.618, 3.618]),
                        
                    limit1_levels: json_config["limit1_levels"]
                        .as_array()
                        .map(|arr| arr.iter().filter_map(|v| v.as_f64()).collect())
                        .unwrap_or_else(|| vec![0.5, 0.618, 1.0, 1.272]),
                        
                    limit2_levels: json_config["limit2_levels"]
                        .as_array()
                        .map(|arr| arr.iter().filter_map(|v| v.as_f64()).collect())
                        .unwrap_or_else(|| vec![1.0, 1.272, 1.618, 2.0]),
                        
                    threshold_factors: json_config["threshold_factors"]
                        .as_array()
                        .map(|arr| arr.iter().filter_map(|v| v.as_f64()).collect())
                        .unwrap_or_else(|| vec![0.75, 1.0, 1.25, 1.5]),
                        
                    output_dir: json_config["output_dir"].as_str().unwrap_or("results").to_string(),
                    parallel: json_config["parallel"].as_bool().unwrap_or(true),
                    num_best_results: json_config["num_best_results"].as_u64().unwrap_or(20) as usize,
                };
            }
        }
        
        // Fallback to the default if file can't be read
        println!("Warning: Could not load optimization_config.json, using default config");
        super::dynamic_optimizer::python_like_optimization_config()
    }
}