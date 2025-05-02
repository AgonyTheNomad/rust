// src/lib.rs
// Main library file that exports all the modules

// Declare all the modules that are part of the public API
pub mod backtest;
pub mod fetch_data;
pub mod indicators;
pub mod models;
pub mod risk;
pub mod strategy;
pub mod optimizer;
pub mod stats;
pub mod metrics;   // Make sure this line is present
pub mod signals;
pub mod influx;

// Re-export key types to make them easier to use from tests and binaries
pub use crate::models::Candle;
pub use crate::strategy::{Strategy, StrategyConfig, AssetConfig};
pub use crate::backtest::Backtester;