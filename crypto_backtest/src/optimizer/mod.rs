// src/optimizer/mod.rs

pub mod fibonacci_optimizer {
    // Re-export everything from dynamic_optimizer under fibonacci_optimizer namespace
    pub use super::dynamic_optimizer::*;
    
    // For backward compatibility, we can alias some types
    pub type FibonacciOptimizer = super::dynamic_optimizer::DynamicFibonacciOptimizer;
    pub type FibonacciOptimizationConfig = super::dynamic_optimizer::DynamicOptimizationConfig;
    pub type OptimizationMetric = &'static str;  // A simple type for compatibility
    pub type OptimizationResult = super::dynamic_optimizer::OptimizationResult;
    
    // Alias for backward compatibility
    pub fn default_fibonacci_optimization_config() -> FibonacciOptimizationConfig {
        super::dynamic_optimizer::python_like_optimization_config()
    }
}

// We're keeping the original grid-based optimizer for compatibility
mod grid_optimizer;
pub use grid_optimizer::{optimize, OptimizationParams};

// Add the dynamic optimizer module
pub mod dynamic_optimizer;

// Re-export for convenience
pub use dynamic_optimizer::{
    DynamicFibonacciOptimizer,
    DynamicOptimizationConfig,
    AssetConfig,
    OptimizationResult,
    optimize_assets_from_config,
};