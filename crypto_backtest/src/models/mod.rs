use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candle {
    pub time: String,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub num_trades: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PositionType {
    Long,
    Short,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PositionStatus {
    Pending,
    Active,
    Closed,
}

#[derive(Debug, Clone)]
pub struct Signal {
    pub id: String,
    pub symbol: String,
    pub timestamp: DateTime<Utc>,
    pub position_type: PositionType,
    pub price: f64,
    pub take_profit: f64,
    pub stop_loss: f64,
    pub reason: String,
    pub strength: f64,
    pub processed: bool,
}

impl Signal {
    pub fn new(
        symbol: String,
        position_type: PositionType,
        price: f64,
        take_profit: f64,
        stop_loss: f64,
        reason: String,
        strength: f64,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            symbol,
            timestamp: Utc::now(),
            position_type,
            price,
            take_profit,
            stop_loss,
            reason,
            strength,
            processed: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Position {
    pub id: String,
    pub symbol: String,
    pub entry_time: String,
    pub entry_price: f64,
    pub size: f64,
    pub stop_loss: f64,
    pub take_profit: f64,
    pub position_type: PositionType,
    pub risk_percent: f64,
    pub margin_used: f64,
    pub status: PositionStatus,

    // Scaling Support
    pub limit1_price: Option<f64>,
    pub limit2_price: Option<f64>,
    pub limit1_hit: bool,
    pub limit2_hit: bool,
    pub limit1_size: f64,
    pub limit2_size: f64,
    pub new_tp1: Option<f64>,
    pub new_tp2: Option<f64>,
    
    // Order IDs
    pub entry_order_id: Option<String>,
    pub tp_order_id: Option<String>,
    pub sl_order_id: Option<String>,
    pub limit1_order_id: Option<String>,
    pub limit2_order_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Trade {
    pub entry_time: String,
    pub exit_time: String,
    pub position_type: String,
    pub entry_price: f64,
    pub exit_price: f64,
    pub size: f64,
    pub pnl: f64,
    pub risk_percent: f64,
    pub profit_factor: f64,
    pub margin_used: f64,
    pub fees: f64,
    pub slippage: f64,
}

#[derive(Debug)]
pub struct BacktestState {
    pub account_balance: f64,
    pub initial_balance: f64,
    pub position: Option<Position>,
    pub equity_curve: Vec<f64>,
    pub trades: Vec<Trade>,
    pub max_drawdown: f64,
    pub peak_balance: f64,
    pub current_drawdown: f64,
}

#[derive(Debug, Clone)]
pub struct Account {
    pub balance: f64,
    pub equity: f64,
    pub used_margin: f64,
    pub positions: HashMap<String, Position>,
}

impl Account {
    pub fn new(initial_balance: f64) -> Self {
        Self {
            balance: initial_balance,
            equity: initial_balance,
            used_margin: 0.0,
            positions: HashMap::new(),
        }
    }

    pub fn available_margin(&self) -> f64 {
        self.equity - self.used_margin
    }
}