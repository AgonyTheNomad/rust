use std::collections::HashMap;
use crate::models::{Account, PositionType};

// Import the position calculator module
mod position_calculator;
pub use position_calculator::{PositionResult, calculate_positions};

#[derive(Debug, Clone)]
pub struct RiskParameters {
    pub max_risk_per_trade: f64,
    pub max_position_size: f64,
    pub max_leverage: f64,
    pub spread: f64,              // Added spread parameter
}

impl Default for RiskParameters {
    fn default() -> Self {
        Self {
            max_risk_per_trade: 0.02,
            max_position_size: 10.0,
            max_leverage: 20.0,
            spread: 0.0003,        // Default spread of 0.03%
        }
    }
}

pub struct RiskManager {
    pub parameters: RiskParameters,
}

impl RiskManager {
    pub fn new(parameters: RiskParameters) -> Self {
        Self { parameters }
    }

    pub fn calculate_positions_with_risk(
        &self,
        account: &Account,
        entry: f64,
        tp: f64,
        sl: f64,
        limit1: f64,
        limit2: f64,
        leverage: f64,
        position_type: PositionType,
    ) -> Result<PositionResult, String> {
        // Apply spread to prices based on position type
        let (entry_with_spread, tp_with_spread, sl_with_spread) = match position_type {
            PositionType::Long => (
                entry * (1.0 + self.parameters.spread), // Higher entry for long
                tp * (1.0 - self.parameters.spread),    // Lower TP for long
                sl * (1.0 + self.parameters.spread),    // Higher SL for long
            ),
            PositionType::Short => (
                entry * (1.0 - self.parameters.spread), // Lower entry for short
                tp * (1.0 + self.parameters.spread),    // Higher TP for short
                sl * (1.0 - self.parameters.spread),    // Lower SL for short
            ),
        };
        
        // Use the position calculator with the spread-adjusted prices
        calculate_positions(
            entry_with_spread,
            tp_with_spread,
            sl_with_spread,
            limit1, 
            limit2,
            account.balance,
            self.parameters.max_risk_per_trade,
            leverage,
            position_type,
            4.0, // h11 default value
            6.0, // h12 default value
        )
    }
}