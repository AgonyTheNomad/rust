use crate::models::{Position, Trade, PositionType, Account};
use crate::influxdb::InfluxDBClient;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;
use std::sync::Arc;
use anyhow::Result;
use std::collections::HashMap;

mod account_reader;
pub use account_reader::{AccountReader, AccountInfo, Position as AccountPosition};

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
    
    // New helper method to check if a symbol has an open position
    async fn has_open_position_for_symbol(&self, symbol: &str) -> Result<bool, ExchangeError> {
        // Get all positions
        let positions = self.get_positions().await?;
        
        // Check if any position matches the symbol and is open
        let has_position = positions.iter().any(|pos| 
            pos.symbol == symbol && 
            pos.status == crate::models::PositionStatus::Open
        );
        
        Ok(has_position)
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
    pub account_reader: Arc<AccountReader>,
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
        // Get balance from the account info file, with no fallback
        match self.account_reader.get_balance() {
            Ok(balance) => Ok(balance),
            Err(e) => {
                // Convert anyhow::Error to ExchangeError
                Err(ExchangeError::ApiError(format!("Failed to read balance: {}", e)))
            }
        }
    }
    
    async fn get_positions(&self) -> Result<Vec<Position>, ExchangeError> {
        // Try to get positions from account info file
        match self.account_reader.read_account_info() {
            Ok(account_info) => {
                let mut positions = Vec::new();
                
                for p in account_info.positions {
                    let position_type = if p.side == "LONG" {
                        PositionType::Long
                    } else {
                        PositionType::Short
                    };
                    
                    positions.push(Position {
                        id: format!("{}_{}", p.symbol, uuid::Uuid::new_v4()),
                        symbol: p.symbol,
                        entry_time: chrono::Utc::now(), // We don't have entry time in the file
                        entry_price: p.entry_price,
                        size: p.size,
                        stop_loss: 0.0, // Not available in account info
                        take_profit: 0.0, // Not available in account info
                        position_type,
                        risk_percent: 0.0, // Not available
                        margin_used: 0.0, // Not available
                        status: crate::models::PositionStatus::Open,
                        limit1_price: None,
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
                    });
                }
                
                Ok(positions)
            },
            Err(e) => {
                log::warn!("Failed to read positions from account_info.json: {}", e);
                Err(ExchangeError::ApiError(format!("Failed to read positions: {}", e)))
            }
        }
    }
    
    async fn get_account_info(&self) -> Result<Account, ExchangeError> {
        // Get account info from file
        match self.account_reader.read_account_info() {
            Ok(account_info) => {
                // Convert positions from file format to our internal format
                let mut positions = HashMap::new();
                for p in &account_info.positions {
                    let position_type = if p.side == "LONG" {
                        PositionType::Long
                    } else {
                        PositionType::Short
                    };
                    
                    let position = Position {
                        id: format!("{}_{}", p.symbol, uuid::Uuid::new_v4()),
                        symbol: p.symbol.clone(),
                        entry_time: chrono::Utc::now(), // We don't have entry time in the file
                        entry_price: p.entry_price,
                        size: p.size,
                        stop_loss: 0.0, // Not available in account info
                        take_profit: 0.0, // Not available in account info
                        position_type,
                        risk_percent: 0.0, // Not available
                        margin_used: 0.0, // Not available
                        status: crate::models::PositionStatus::Open,
                        limit1_price: None,
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
                    };
                    
                    positions.insert(position.id.clone(), position);
                }
                
                Ok(Account {
                    balance: account_info.balance,
                    equity: account_info.balance, // Use balance as equity
                    used_margin: account_info.used_margin,
                    positions,
                })
            },
            Err(e) => {
                log::warn!("Failed to read account info from account_info.json: {}", e);
                // Return error instead of creating a mock account
                Err(ExchangeError::ApiError(format!("Failed to read account info: {}", e)))
            }
        }
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
        // Create a copy of the position with updated status
        let mut updated_position = position.clone();
        updated_position.status = crate::models::PositionStatus::Open;
        
        Ok(updated_position) // Return the updated position
    }
    
    async fn close_position(&self, _position_id: &str) -> Result<Trade, ExchangeError> {
        Err(ExchangeError::ApiError("Mock exchange not implemented".to_string()))
    }
    
    async fn update_position(&self, position: &Position) -> Result<Position, ExchangeError> {
        Ok(position.clone()) // Just return a clone of the position
    }
    
    async fn has_open_position_for_symbol(&self, symbol: &str) -> Result<bool, ExchangeError> {
        // Check if symbol has an open position from account info file
        match self.account_reader.has_open_position(symbol) {
            Ok(has_position) => Ok(has_position),
            Err(e) => {
                log::warn!("Failed to check if symbol has open position: {}", e);
                // Fall back to the default implementation
                let positions = self.get_positions().await?;
                
                let has_position = positions.iter().any(|pos| 
                    pos.symbol == symbol && 
                    pos.status == crate::models::PositionStatus::Open
                );
                
                Ok(has_position)
            }
        }
    }
}

// Factory function to create exchange instances
pub fn create_exchange(config: ExchangeConfig) -> Result<Box<dyn Exchange>, ExchangeError> {
    let influx_config = crate::influxdb::InfluxDBConfig::from_env()
        .map_err(|e| ExchangeError::ApiError(format!("Failed to load InfluxDB config: {}", e)))?;
    
    let influx_client = crate::influxdb::InfluxDBClient::new(influx_config)
        .map_err(|e| ExchangeError::ApiError(format!("Failed to create InfluxDB client: {}", e)))?;
    
    // Create account reader with 120-second max age
    let account_reader = Arc::new(AccountReader::new(
        "./account_info.json",
        120
    ));
    
    let exchange = MockExchange {
        name: config.name,
        influx: Arc::new(influx_client),
        account_reader,
    };
    
    Ok(Box::new(exchange))
}