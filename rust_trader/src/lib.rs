pub mod config;
pub mod exchange;
pub mod influxdb;
pub mod models;
pub mod risk;
pub mod signals;
pub mod strategy;

// Re-export commonly used types
pub use crate::models::{Candle, Position, PositionType, Trade};
pub use crate::exchange::{Exchange, ExchangeError, OrderStatus, OrderType, OrderSide};
pub use crate::influxdb::{InfluxDBClient, InfluxDBConfig};
pub use crate::strategy::{Strategy, StrategyConfig, AssetConfig};
pub use crate::risk::{RiskManager, RiskParameters, PositionCalculator};

use tracing_subscriber::{fmt, EnvFilter};
use log::{error, info, warn, debug};

pub fn setup_logging() {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,rust_trader=debug"));

    fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .init();
    
    info!("Logging initialized");
}