use crate::models::{Position, Trade, PositionType, Account, Signal};
use crate::influxdb::InfluxDBClient;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;
use std::sync::Arc;
use anyhow::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeConfig {
    pub name: String,
    pub api_key: String,
    pub api_secret: String,
    pub base_url: String,
    pub websocket_url: String,
    pub testnet: bool,
    pub additional_params: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrderType {
    Market,
    Limit,
    StopLoss,
    TakeProfit,
    TrailingStop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrderSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrderStatus {
    New,
    PartiallyFilled,
    Filled,
    Canceled,
    Rejected,
    Expired,
}

#[derive(Debug, Error)]
pub enum ExchangeError {
    #[error("API error: {0}")]
    ApiError(String),
    
    #[error("Authentication error: {0}")]
    AuthError(String),
    
    #[error("Rate limit exceeded: {0}")]
    RateLimitExceeded(String),
    
    #[error("Insufficient funds: {0}")]
    InsufficientFunds(String),
    
    #[error("Invalid order: {0}")]
    InvalidOrder(String),
    
    #[error("Order not found: {0}")]
    OrderNotFound(String),
    
    #[error("Network error: {0}")]
    NetworkError(String),
    
    #[error("Parse error: {0}")]
    ParseError(String),
    
    #[error("Unknown error: {0}")]
    UnknownError(String),
}

// Implement Display trait for enum types
impl fmt::Display for OrderType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OrderType::Market => write!(f, "Market"),
            OrderType::Limit => write!(f, "Limit"),
            OrderType::StopLoss => write!(f, "StopLoss"),
            OrderType::TakeProfit => write!(f, "TakeProfit"),
            OrderType::TrailingStop => write!(f, "TrailingStop"),
        }
    }
}

impl fmt::Display for OrderSide {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OrderSide::Buy => write!(f, "Buy"),
            OrderSide::Sell => write!(f, "Sell"),
        }
    }
}

impl fmt::Display for OrderStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OrderStatus::New => write!(f, "New"),
            OrderStatus::PartiallyFilled => write!(f, "PartiallyFilled"),
            OrderStatus::Filled => write!(f, "Filled"),
            OrderStatus::Canceled => write!(f, "Canceled"),
            OrderStatus::Rejected => write!(f, "Rejected"),
            OrderStatus::Expired => write!(f, "Expired"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: String,
    pub exchange_id: Option<String>,
    pub symbol: String,
    pub order_type: OrderType,
    pub side: OrderSide,
    pub price: Option<f64>,
    pub amount: f64,
    pub filled_amount: f64,
    pub status: OrderStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
    pub position_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBook {
    pub symbol: String,
    pub bids: Vec<(f64, f64)>, // (price, quantity)
    pub asks: Vec<(f64, f64)>, // (price, quantity)
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

// Add trait for cloning boxed Exchange objects
pub trait CloneBox {
    fn clone_box(&self) -> Box<dyn Exchange>;
}

// This is the trait that needs to be implemented by exchange clients
#[async_trait]
pub trait Exchange: Send + Sync + CloneBox {
    async fn get_name(&self) -> &str;
    
    // Market data methods
    async fn get_ticker(&self, symbol: &str) -> Result<f64, ExchangeError>;
    async fn get_order_book(&self, symbol: &str, depth: Option<usize>) -> Result<OrderBook, ExchangeError>;
    
    // Account methods
    async fn get_balance(&self) -> Result<f64, ExchangeError>;
    async fn get_positions(&self) -> Result<Vec<Position>, ExchangeError>;
    async fn get_account_info(&self) -> Result<Account, ExchangeError>;
    
    // Order methods
    async fn create_order(&self, order: Order) -> Result<Order, ExchangeError>;
    async fn cancel_order(&self, order_id: &str, symbol: &str) -> Result<(), ExchangeError>;
    async fn get_order(&self, order_id: &str, symbol: &str) -> Result<Order, ExchangeError>;
    
    // Position methods
    async fn open_position(&self, position: &Position) -> Result<Position, ExchangeError>;
    async fn close_position(&self, position_id: &str) -> Result<Trade, ExchangeError>;
    async fn update_position(&self, position: &Position) -> Result<Position, ExchangeError>;
    
    // Helper methods
    fn position_type_to_order_side(&self, position_type: &PositionType) -> OrderSide {
        match position_type {
            PositionType::Long => OrderSide::Buy,
            PositionType::Short => OrderSide::Sell,
        }
    }
    
    fn close_position_side(&self, position_type: &PositionType) -> OrderSide {
        match position_type {
            PositionType::Long => OrderSide::Sell,
            PositionType::Short => OrderSide::Buy,
        }
    }
}

// Implement the CloneBox trait for any type that implements Exchange and Clone
impl<T> CloneBox for T
where
    T: 'static + Exchange + Clone,
{
    fn clone_box(&self) -> Box<dyn Exchange> {
        Box::new(self.clone())
    }
}

// Stub implementation for the exchange
#[derive(Clone)]
pub struct MockExchange {
    pub name: String,
    pub influx: Arc<InfluxDBClient>,
}

#[async_trait]
impl Exchange for MockExchange {
    async fn get_name(&self) -> &str {
        &self.name
    }
    
    async fn get_ticker(&self, _symbol: &str) -> Result<f64, ExchangeError> {
        // Return a dummy price for dry run
        Ok(10000.0)
    }
    
    async fn get_order_book(&self, _symbol: &str, _depth: Option<usize>) -> Result<OrderBook, ExchangeError> {
        Err(ExchangeError::ApiError("Mock exchange not implemented".to_string()))
    }
    
    async fn get_balance(&self) -> Result<f64, ExchangeError> {
        // For dry run testing, return a dummy balance
        Ok(10000.0)
    }
    
    async fn get_positions(&self) -> Result<Vec<Position>, ExchangeError> {
        Ok(Vec::new()) // Return empty positions list for dry run
    }
    
    async fn get_account_info(&self) -> Result<Account, ExchangeError> {
        // Create a mock account
        Ok(Account {
            balance: 10000.0,
            equity: 10000.0,
            used_margin: 0.0,
            positions: std::collections::HashMap::new(),
        })
    }
    
    async fn create_order(&self, order: Order) -> Result<Order, ExchangeError> {
        Ok(order) // Just return the same order
    }
    
    async fn cancel_order(&self, _order_id: &str, _symbol: &str) -> Result<(), ExchangeError> {
        Ok(())
    }
    
    async fn get_order(&self, _order_id: &str, _symbol: &str) -> Result<Order, ExchangeError> {
        Err(ExchangeError::ApiError("Mock exchange not implemented".to_string()))
    }
    
    async fn open_position(&self, position: &Position) -> Result<Position, ExchangeError> {
        Ok(position.clone()) // Just return a clone of the position
    }
    
    async fn close_position(&self, _position_id: &str) -> Result<Trade, ExchangeError> {
        Err(ExchangeError::ApiError("Mock exchange not implemented".to_string()))
    }
    
    async fn update_position(&self, position: &Position) -> Result<Position, ExchangeError> {
        Ok(position.clone()) // Just return a clone of the position
    }
}

// Factory function to create exchange instances
pub fn create_exchange(config: ExchangeConfig) -> Result<Box<dyn Exchange>, ExchangeError> {
    let influx_config = crate::influxdb::InfluxDBConfig::from_env()
        .map_err(|e| ExchangeError::ApiError(format!("Failed to load InfluxDB config: {}", e)))?;
    
    let influx_client = crate::influxdb::InfluxDBClient::new(influx_config)
        .map_err(|e| ExchangeError::ApiError(format!("Failed to create InfluxDB client: {}", e)))?;
    
    let exchange = MockExchange {
        name: config.name,
        influx: Arc::new(influx_client),
    };
    
    Ok(Box::new(exchange))
}