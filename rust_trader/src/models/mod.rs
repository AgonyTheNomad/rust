// src/models/mod.rs
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PositionType {
    Long,
    Short,
}

#[derive(Debug, Clone)]
pub struct Signal {
    pub id: String,
    pub symbol: String,
    pub timestamp: DateTime<Utc>,
    pub position_type: PositionType,
    pub price: f64,
    pub reason: String,
    pub strength: f64,
    pub take_profit: f64,
    pub stop_loss: f64,
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
            reason,
            strength,
            take_profit,
            stop_loss,
            processed: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PositionStatus {
    Pending,
    Open,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExitReason {
    TakeProfit,
    StopLoss,
    ManualClose,
    TimeExpiry,
    Error,
}

#[derive(Debug, Clone)]
pub struct Position {
    pub id: String,
    pub symbol: String,
    pub entry_time: DateTime<Utc>,
    pub entry_price: f64,
    pub size: f64,
    pub stop_loss: f64,
    pub take_profit: f64,
    pub position_type: PositionType,
    pub risk_percent: f64,
    pub margin_used: f64,
    pub status: PositionStatus,
    pub limit1_price: Option<f64>,
    pub limit2_price: Option<f64>,
    pub limit1_hit: bool,
    pub limit2_hit: bool,
    pub limit1_size: f64,
    pub limit2_size: f64,
    pub new_tp1: Option<f64>,
    pub new_tp2: Option<f64>,
    pub entry_order_id: Option<String>,
    pub tp_order_id: Option<String>,
    pub sl_order_id: Option<String>,
    pub limit1_order_id: Option<String>,
    pub limit2_order_id: Option<String>,
}

// Add to src/models/mod.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub id: String,
    pub position_id: String,
    pub symbol: String,
    pub entry_time: DateTime<Utc>,
    pub exit_time: DateTime<Utc>,
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
    pub exit_reason: ExitReason,
    // Add these new fields
    pub limit1_price: Option<f64>,
    pub limit2_price: Option<f64>,
    pub limit1_hit: bool,
    pub limit2_hit: bool,
    pub tp1_price: Option<f64>,
    pub tp2_price: Option<f64>,
}

impl Trade {
    pub fn from_position(position: &Position, exit_price: f64, exit_reason: ExitReason) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            position_id: position.id.clone(),
            symbol: position.symbol.clone(),
            entry_time: position.entry_time,
            exit_time: Utc::now(),
            position_type: format!("{:?}", position.position_type),
            entry_price: position.entry_price,
            exit_price,
            size: position.size,
            pnl: position.current_pnl(exit_price),
            risk_percent: position.risk_percent,
            profit_factor: if position.current_pnl(exit_price) > 0.0 { 
                position.current_pnl(exit_price) / (position.size * position.entry_price) 
            } else { 
                0.0 
            },
            margin_used: position.margin_used,
            fees: 0.0,
            slippage: 0.0,
            exit_reason,
            // Add limit and TP levels
            limit1_price: position.limit1_price,
            limit2_price: position.limit2_price,
            limit1_hit: position.limit1_hit,
            limit2_hit: position.limit2_hit,
            tp1_price: position.new_tp1,
            tp2_price: position.new_tp2,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Account {
    pub balance: f64,
    pub equity: f64,
    pub used_margin: f64,
    pub positions: HashMap<String, Position>,
}

impl Position {
    // Status helper methods
    pub fn mark_as_open(&mut self) {
        self.status = PositionStatus::Open;
    }
    
    pub fn mark_as_closed(&mut self) {
        self.status = PositionStatus::Closed;
    }
    
    pub fn is_open(&self) -> bool {
        self.status == PositionStatus::Open
    }
    
    pub fn is_pending(&self) -> bool {
        self.status == PositionStatus::Pending
    }
    
    pub fn is_closed(&self) -> bool {
        self.status == PositionStatus::Closed
    }

    pub fn current_pnl(&self, current_price: f64) -> f64 {
        match self.position_type {
            PositionType::Long => (current_price - self.entry_price) * self.size,
            PositionType::Short => (self.entry_price - current_price) * self.size,
        }
    }

    pub fn is_hit_limit1(&self, current_price: f64) -> bool {
        if let Some(limit1) = self.limit1_price {
            match self.position_type {
                PositionType::Long => current_price <= limit1,
                PositionType::Short => current_price >= limit1,
            }
        } else {
            false
        }
    }

    pub fn is_hit_limit2(&self, current_price: f64) -> bool {
        if let Some(limit2) = self.limit2_price {
            match self.position_type {
                PositionType::Long => current_price <= limit2,
                PositionType::Short => current_price >= limit2,
            }
        } else {
            false
        }
    }
}

impl Account {
    pub fn available_margin(&self) -> f64 {
        self.equity - self.used_margin
    }
}