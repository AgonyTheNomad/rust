use crate::models::{Position, Trade, PositionType};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;
use anyhow::Result;
use log::*;

pub mod hyperliquid;

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

impl ExchangeConfig {
    pub fn from_env(exchange_name: &str) -> Result<Self> {
        dotenv::dotenv().ok(); // Load from .env file if available
        
        let env_prefix = exchange_name.to_uppercase();
        
        let api_key = std::env::var(format!("{}_API_KEY", env_prefix))
            .map_err(|_| anyhow::anyhow!("{}_API_KEY environment variable not set", env_prefix))?;
            
        let api_secret = std::env::var(format!("{}_API_SECRET", env_prefix))
            .map_err(|_| anyhow::anyhow!("{}_API_SECRET environment variable not set", env_prefix))?;
            
        let base_url = std::env::var(format!("{}_BASE_URL", env_prefix))
            .unwrap_or_else(|_| match exchange_name.to_lowercase().as_str() {
                "hyperliquid" => "https://api.hyperliquid.xyz".to_string(),
                "binance" => "https://api.binance.com".to_string(),
                _ => "".to_string(),
            });
            
        let websocket_url = std::env::var(format!("{}_WEBSOCKET_URL", env_prefix))
            .unwrap_or_else(|_| match exchange_name.to_lowercase().as_str() {
                "hyperliquid" => "wss://api.hyperliquid.xyz/ws".to_string(),
                "binance" => "wss://stream.binance.com:9443/ws".to_string(),
                _ => "".to_string(),
            });
            
        let testnet = std::env::var(format!("{}_TESTNET", env_prefix))
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(false);
            
        // Collect any additional parameters
        let mut additional_params = std::collections::HashMap::new();
        
        // For example, some exchanges might need specific parameters
        if let Ok(value) = std::env::var(format!("{}_PASSPHRASE", env_prefix)) {
            additional_params.insert("passphrase".to_string(), value);
        }
        
        Ok(Self {
            name: exchange_name.to_string(),
            api_key,
            api_secret,
            base_url,
            websocket_url,
            testnet,
            additional_params,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrderType {
    Market,
    Limit,
    StopLoss,
    TakeProfit,
    TrailingStop,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrderSide {
    Buy,
    Sell,
}

impl fmt::Display for OrderSide {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OrderSide::Buy => write!(f, "Buy"),
            OrderSide::Sell => write!(f, "Sell"),
        }
    }
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

impl Order {
    pub fn new(
        symbol: String,
        order_type: OrderType,
        side: OrderSide,
        price: Option<f64>,
        amount: f64,
        position_id: Option<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            exchange_id: None,
            symbol,
            order_type,
            side,
            price,
            amount,
            filled_amount: 0.0,
            status: OrderStatus::New,
            created_at: chrono::Utc::now(),
            updated_at: None,
            position_id,
        }
    }

    pub fn is_complete(&self) -> bool {
        matches!(self.status, OrderStatus::Filled | OrderStatus::Canceled | OrderStatus::Rejected | OrderStatus::Expired)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBook {
    pub symbol: String,
    pub bids: Vec<(f64, f64)>, // (price, quantity)
    pub asks: Vec<(f64, f64)>, // (price, quantity)
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl OrderBook {
    pub fn best_bid(&self) -> Option<f64> {
        self.bids.first().map(|(price, _)| *price)
    }

    pub fn best_ask(&self) -> Option<f64> {
        self.asks.first().map(|(price, _)| *price)
    }

    pub fn mid_price(&self) -> Option<f64> {
        match (self.best_bid(), self.best_ask()) {
            (Some(bid), Some(ask)) => Some((bid + ask) / 2.0),
            _ => None,
        }
    }
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

#[async_trait]
pub trait Exchange: Send + Sync {
    async fn get_name(&self) -> &str;
    
    // Market data methods
    async fn get_ticker(&self, symbol: &str) -> Result<f64, ExchangeError>;
    async fn get_order_book(&self, symbol: &str, depth: Option<usize>) -> Result<OrderBook, ExchangeError>;
    
    // Account methods
    async fn get_balance(&self) -> Result<f64, ExchangeError>;
    async fn get_positions(&self) -> Result<Vec<Position>, ExchangeError>;
    
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

// Factory function to create exchange instances
pub fn create_exchange(config: ExchangeConfig) -> Result<Box<dyn Exchange>> {
    match config.name.to_lowercase().as_str() {
        "hyperliquid" => {
            let exchange = hyperliquid::HyperliquidExchange::new(config)?;
            Ok(Box::new(exchange))
        },
        // Add other exchanges as they're implemented
        _ => Err(anyhow::anyhow!("Unsupported exchange: {}", config.name)),
    }
}