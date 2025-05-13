use crate::models::{Account, PositionType};
use anyhow::Result;
use log::*;
use serde::{Deserialize, Serialize};

mod position_calculator;
pub use position_calculator::{PositionCalculator, PositionResult, PositionScaleResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskParameters {
    pub max_risk_per_trade: f64,
    pub max_position_size: f64,
    pub max_leverage: f64,
    pub spread: f64,
    pub max_open_positions: usize,
    pub max_drawdown: f64,
    pub max_daily_loss: f64,
    pub kelly_fraction: f64,
}

impl Default for RiskParameters {
    fn default() -> Self {
        Self {
            max_risk_per_trade: 0.02,       // 2% per trade
            max_position_size: 10.0,        // 10 units maximum position
            max_leverage: 20.0,             // 20x leverage
            spread: 0.0003,                 // 0.03% spread
            max_open_positions: 5,          // Max 5 positions open at once
            max_drawdown: 0.20,             // 20% max drawdown
            max_daily_loss: 0.05,           // 5% daily loss limit
            kelly_fraction: 0.5,            // Half Kelly criterion
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyStats {
    pub date: chrono::NaiveDate,
    pub starting_balance: f64,
    pub current_balance: f64,
    pub daily_pnl: f64,
    pub trades_taken: usize,
    pub winning_trades: usize,
    pub losing_trades: usize,
}

impl DailyStats {
    pub fn new(starting_balance: f64) -> Self {
        let today = chrono::Utc::now().date_naive();
        
        Self {
            date: today,
            starting_balance,
            current_balance: starting_balance,
            daily_pnl: 0.0,
            trades_taken: 0,
            winning_trades: 0,
            losing_trades: 0,
        }
    }
    
    pub fn update_with_trade(&mut self, pnl: f64) {
        self.daily_pnl += pnl;
        self.current_balance += pnl;
        self.trades_taken += 1;
        
        if pnl > 0.0 {
            self.winning_trades += 1;
        } else if pnl < 0.0 {
            self.losing_trades += 1;
        }
    }
    
    pub fn win_rate(&self) -> f64 {
        if self.trades_taken == 0 {
            return 0.0;
        }
        
        self.winning_trades as f64 / self.trades_taken as f64
    }
    
    pub fn daily_return(&self) -> f64 {
        self.daily_pnl / self.starting_balance
    }
    
    pub fn is_new_day(&self) -> bool {
        let today = chrono::Utc::now().date_naive();
        self.date != today
    }
    
    pub fn reset_for_new_day(&mut self) {
        let today = chrono::Utc::now().date_naive();
        
        self.date = today;
        self.starting_balance = self.current_balance;
        self.daily_pnl = 0.0;
        self.trades_taken = 0;
        self.winning_trades = 0;
        self.losing_trades = 0;
    }
}

pub struct RiskManager {
    pub parameters: RiskParameters,
    pub position_calculator: PositionCalculator,
    pub daily_stats: DailyStats,
    pub max_drawdown_equity: f64,
    pub current_drawdown: f64,
}

impl RiskManager {
    pub fn new(parameters: RiskParameters, initial_balance: f64) -> Self {
        Self {
            parameters,
            position_calculator: PositionCalculator::new(),
            daily_stats: DailyStats::new(initial_balance),
            max_drawdown_equity: initial_balance,
            current_drawdown: 0.0,
        }
    }
    
    pub fn can_open_new_position(&self, account: &Account) -> bool {
        // Check if we have too many positions open
        if account.positions.len() >= self.parameters.max_open_positions {
            debug!("Can't open new position: max positions limit reached ({}/{})", 
                account.positions.len(), self.parameters.max_open_positions);
            return false;
        }
        
        // Check if we've hit daily loss limit
        if self.daily_stats.daily_return() <= -self.parameters.max_daily_loss {
            warn!("Can't open new position: daily loss limit reached ({}%)", 
                self.daily_stats.daily_return() * 100.0);
            return false;
        }
        
        // Check if we've hit max drawdown
        if self.current_drawdown >= self.parameters.max_drawdown {
            warn!("Can't open new position: max drawdown reached ({}%)", 
                self.current_drawdown * 100.0);
            return false;
        }
        
        true
    }
    
    pub fn calculate_position_size(&self, account: &Account, entry: f64, stop_loss: f64, position_type: PositionType) -> Result<PositionResult> {
        // Get current risk per trade (could be dynamically adjusted based on recent performance)
        let risk = self.determine_risk_per_trade();
        
        // Calculate risk amount in dollars
        let risk_amount = account.balance * risk;
        
        // Calculate distance to stop loss
        let stop_distance = match position_type {
            PositionType::Long => (entry - stop_loss).abs(),
            PositionType::Short => (stop_loss - entry).abs(),
        };
        
        // Calculate position size based on risk and stop distance
        let raw_size = risk_amount / stop_distance;
        
        // Apply Kelly criterion if we have historical data
        let kelly_adjusted_size = if self.daily_stats.trades_taken > 10 {
            let win_rate = self.daily_stats.win_rate();
            let avg_win = 0.0; // Would need to calculate from historical data
            let avg_loss = 0.0; // Would need to calculate from historical data
            
            if avg_loss != 0.0 {
                // Kelly formula: f* = (p * b - q) / b
                // where p = win probability, q = loss probability, b = win/loss ratio
                let win_loss_ratio = if avg_loss != 0.0 { avg_win / avg_loss } else { 1.0 };
                let kelly = (win_rate * win_loss_ratio - (1.0 - win_rate)) / win_loss_ratio;
                
                // Apply Kelly fraction to avoid over-betting
                let kelly_size = raw_size * kelly * self.parameters.kelly_fraction;
                
                // Kelly can return negative values for unfavorable bets
                kelly_size.max(0.0)
            } else {
                raw_size
            }
        } else {
            raw_size
        };
        
        // Apply max position size constraint
        let final_size = kelly_adjusted_size.min(self.parameters.max_position_size);
        
        // Calculate expected margin requirements
        let margin_required = final_size * entry / self.parameters.max_leverage;
        
        // Check if account has enough margin
        if margin_required > account.available_margin() {
            return Err(anyhow::anyhow!("Insufficient margin: required ${:.2}, available ${:.2}", 
                margin_required, account.available_margin()));
        }
        
        Ok(PositionResult {
            size: final_size,
            risk_amount,
            margin_required,
        })
    }
    
    // New method to calculate scaled position with limits
    pub fn calculate_scaled_position(
        &self,
        account: &Account,
        entry: f64,
        stop_loss: f64,
        take_profit: f64,
        limit1: f64,
        limit2: f64,
        position_type: PositionType
    ) -> Result<PositionScaleResult> {
        // Get the risk level
        let risk = self.determine_risk_per_trade();
        
        // Use the position calculator to calculate scaling
        self.position_calculator.calculate_position_scaling(
            entry,
            take_profit,
            stop_loss,
            limit1,
            limit2,
            account.balance,
            risk,
            self.parameters.max_leverage,
            position_type,
        )
    }
    
    pub fn determine_risk_per_trade(&self) -> f64 {
        // Base risk is from parameters
        let mut risk = self.parameters.max_risk_per_trade;
        
        // If we're in drawdown, reduce risk
        if self.current_drawdown > 0.05 {
            // Linear reduction: 5% drawdown = full risk, 20% drawdown = 25% of risk
            let drawdown_factor = 1.0 - ((self.current_drawdown - 0.05) / 0.15);
            risk *= drawdown_factor.max(0.25).min(1.0);
        }
        
        // If we've had consecutive losses, reduce risk further
        // (would need to track this data)
        
        risk
    }
    
    pub fn update_with_trade(&mut self, pnl: f64, current_equity: f64) {
        // Check if it's a new day
        if self.daily_stats.is_new_day() {
            self.daily_stats.reset_for_new_day();
        }
        
        // Update daily stats
        self.daily_stats.update_with_trade(pnl);
        
        // Update drawdown tracking
        if current_equity > self.max_drawdown_equity {
            self.max_drawdown_equity = current_equity;
            self.current_drawdown = 0.0;
        } else {
            self.current_drawdown = (self.max_drawdown_equity - current_equity) / self.max_drawdown_equity;
        }
    }
}