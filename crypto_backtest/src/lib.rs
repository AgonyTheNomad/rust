pub mod backtest;
pub mod fetch_data;
pub mod indicators;
pub mod models;
pub mod risk;
pub mod strategy;
pub mod optimizer;
pub mod stats;
pub mod config;

// Re-export key types to make them easier to use from tests
pub use crate::models::Candle;
pub use crate::strategy::{Strategy, StrategyConfig};
pub use crate::backtest::Backtester;