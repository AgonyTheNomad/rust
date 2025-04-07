use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use crate::models::{Position, Trade, Account, PositionType};

/// Parameters for risk management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskParameters {
    pub max_risk_per_trade: f64,
    pub max_risk_per_day: f64,
    pub max_position_size: f64,
    pub max_leverage: f64,
    pub max_open_positions: usize,
    pub min_risk_reward_ratio: f64,
    pub max_correlation: f64,
    pub margin_call_level: f64,
    pub liquidation_level: f64,
    pub trailing_stop_activation: f64,
    pub trailing_stop_distance: f64,
}

impl Default for RiskParameters {
    fn default() -> Self {
        Self {
            max_risk_per_trade: 0.02,
            max_risk_per_day: 0.06,
            max_position_size: 10.0,
            max_leverage: 20.0,
            max_open_positions: 3,
            min_risk_reward_ratio: 2.0,
            max_correlation: 0.7,
            margin_call_level: 0.8,
            liquidation_level: 0.9,
            trailing_stop_activation: 0.01,
            trailing_stop_distance: 0.005,
        }
    }
}

/// Metrics to track the risk state
#[derive(Debug, Clone, Default)]
pub struct RiskMetrics {
    pub current_drawdown: f64,
    pub max_drawdown: f64,
    pub daily_loss: f64,
    pub open_risk: f64,
    pub margin_usage: f64,
    pub position_concentration: f64,
    pub value_at_risk: f64,
}

/// Result of position calculations
#[derive(Debug)]
pub struct PositionResult {
    pub initial_position_size: f64,
    pub limit1_position_size: f64,
    pub limit2_position_size: f64,
    pub new_tp1: f64,
    pub new_tp2: f64,
    pub max_margin: f64,
    pub final_risk: f64,
}

/// Manages risk metrics and checks
pub struct RiskManager {
    pub parameters: RiskParameters,
    daily_risk_tracker: HashMap<String, f64>,
    position_correlations: HashMap<String, f64>,
    risk_metrics: RiskMetrics,
}

impl RiskManager {
    /// Create a new RiskManager
    pub fn new(parameters: RiskParameters) -> Self {
        Self {
            parameters,
            daily_risk_tracker: HashMap::new(),
            position_correlations: HashMap::new(),
            risk_metrics: RiskMetrics::default(),
        }
    }

    /// Calculate positions with multiple limits
    pub fn calculate_positions_with_risk(
        &self,
        account: &Account,
        initial: f64,
        tp: f64,
        sl: f64,
        limit_1: f64,
        limit_2: f64,
        leverage: f64,
    ) -> Result<PositionResult, RiskError> {
        let risk = self.parameters.max_risk_per_trade;

        let result = calculate_positions(
            initial,
            tp,
            sl,
            limit_1,
            limit_2,
            account.balance,
            risk,
            leverage,
            4.0, // h11
            6.0, // h12
            "LONG",
        );

        if result.max_margin > self.parameters.margin_call_level {
            return Err(RiskError::ExceedsMaxPositionSize);
        }

        Ok(result)
    }

    /// Validate new position against risk parameters
    pub fn validate_new_position(
        &self,
        account: &Account,
        position: &Position,
    ) -> Result<(), RiskError> {
        if account.positions.len() >= self.parameters.max_open_positions {
            return Err(RiskError::TooManyOpenPositions);
        }

        let risk = match position.position_type {
            PositionType::Long => (position.entry_price - position.stop_loss) * position.size,
            PositionType::Short => (position.stop_loss - position.entry_price) * position.size,
        };

        let reward = match position.position_type {
            PositionType::Long => (position.take_profit - position.entry_price) * position.size,
            PositionType::Short => (position.entry_price - position.take_profit) * position.size,
        };

        let risk_reward_ratio = reward / risk;
        if risk_reward_ratio < self.parameters.min_risk_reward_ratio {
            return Err(RiskError::InsufficientRiskRewardRatio);
        }

        Ok(())
    }

    /// Update risk metrics based on current state
    pub fn update_risk_metrics(
        &mut self,
        account: &Account,
        trades: &[Trade],
        current_time: DateTime<Utc>,
    ) {
        let peak_balance = trades.iter()
            .map(|t| t.pnl)
            .fold(account.balance, |peak, pnl| peak.max(peak + pnl));
        
        self.risk_metrics.current_drawdown = if peak_balance > 0.0 {
            (peak_balance - account.balance) / peak_balance
        } else {
            0.0
        };

        self.risk_metrics.max_drawdown = self.risk_metrics.max_drawdown
            .max(self.risk_metrics.current_drawdown);

        let today = current_time.format("%Y-%m-%d").to_string();
        self.risk_metrics.daily_loss = trades
                .iter()
                .filter(|t| t.exit_time.starts_with(&today) && t.pnl < 0.0)
                .map(|t| t.pnl.abs())
                .sum();

        self.risk_metrics.open_risk = account.positions.values()
            .map(|p| (p.entry_price - p.stop_loss).abs() * p.size)
            .sum();

        self.risk_metrics.margin_usage = account.used_margin / account.equity;

        let total_position_value = account.positions.values()
            .map(|p| p.size * p.entry_price)
            .sum::<f64>();
        
        self.risk_metrics.position_concentration = if total_position_value > 0.0 {
            account.positions.values()
                .map(|p| (p.size * p.entry_price) / total_position_value)
                .fold(0.0, |max, concentration| max.max(concentration))
        } else {
            0.0
        };

        self.calculate_value_at_risk(trades);
    }

    fn calculate_value_at_risk(&mut self, trades: &[Trade]) {
        if trades.len() < 30 {
            return;
        }

        let returns: Vec<f64> = trades.iter()
            .map(|t| t.pnl / t.size)
            .collect();

        let mean = returns.iter().sum::<f64>() / returns.len() as f64;
        let variance = returns.iter()
            .map(|r| (r - mean).powi(2))
            .sum::<f64>() / returns.len() as f64;
        let std_dev = variance.sqrt();

        self.risk_metrics.value_at_risk = mean - (1.96 * std_dev);
    }
}

/// Errors related to risk management
#[derive(Debug, thiserror::Error)]
pub enum RiskError {
    #[error("Position size exceeds maximum allowed")]
    ExceedsMaxPositionSize,
    
    #[error("Leverage exceeds maximum allowed")]
    ExceedsMaxLeverage,
    
    #[error("Insufficient margin available")]
    InsufficientMargin,
    
    #[error("Too many open positions")]
    TooManyOpenPositions,
    
    #[error("Risk/Reward ratio below minimum")]
    InsufficientRiskRewardRatio,
}

/// Calculate positions
pub fn calculate_positions(
    initial: f64,
    tp: f64,
    sl: f64,
    limit_1: f64,
    limit_2: f64,
    account_size: f64,
    mut risk: f64,
    leverage: f64,
    h11: f64,
    h12: f64,
    position_type: &str,
) -> PositionResult {
    let mut max_margin = 0.0;

    let calculate_e11 = |a11: f64, d7: f64, h11: f64| -> f64 {
        let ratio = (d7 / h11) * a11;
        if position_type.eq_ignore_ascii_case("LONG") {
            a11 + ratio
        } else {
            a11 - ratio
        }
    };

    let calculate_d8 = |g6: f64, a12: f64, sl: f64| -> f64 {
        if position_type.eq_ignore_ascii_case("LONG") {
            g6 / (a12 - sl)
        } else {
            g6 / (sl - a12)
        }
    };

    let calculate_e12 = |a12: f64, d7: f64, h12: f64| -> f64 {
        let ratio = (d7 / h12) * a12;
        if position_type.eq_ignore_ascii_case("LONG") {
            a12 + ratio
        } else {
            a12 - ratio
        }
    };

    let mut d5 = 0.0;
    let mut d11 = 0.0;
    let mut d12 = 0.0;
    let mut new_tp1 = 0.0;
    let mut new_tp2 = 0.0;

    loop {
        let g6 = risk * account_size;
        let a11 = (initial + (limit_1 * 3.0)) / 4.0;
        let a12 = (initial + (limit_1 * 3.0) + (limit_2 * 5.0)) / 9.0;
        let e8 = if position_type.eq_ignore_ascii_case("LONG") {
            tp - initial
        } else {
            initial - tp
        };
        let d7 = e8 / initial;

        let d8 = calculate_d8(g6, a12, sl);
        new_tp1 = calculate_e11(a11, d7, h11);
        new_tp2 = calculate_e12(a12, d7, h12);

        d5 = d8 / 9.0;
        let i11 = d5 * 4.0;

        d11 = d5 * 3.0;
        d12 = d5 * 5.0;

        max_margin = (a12 * i11) / (account_size * leverage * 0.6);


        if max_margin <= 1.0 {
            break;
        } else {
            risk -= 0.01;
        }
    }

    PositionResult {
        initial_position_size: d5,
        limit1_position_size: d11,
        limit2_position_size: d12,
        new_tp1,
        new_tp2,
        max_margin,
        final_risk: risk,
    }
}
