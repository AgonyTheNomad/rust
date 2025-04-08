use std::collections::HashMap;
use crate::models::Account;

#[derive(Debug, Clone)]
pub struct RiskParameters {
    pub max_risk_per_trade: f64,
    pub max_position_size: f64,
    pub max_leverage: f64,
}

impl Default for RiskParameters {
    fn default() -> Self {
        Self {
            max_risk_per_trade: 0.02,
            max_position_size: 10.0,
            max_leverage: 20.0,
        }
    }
}

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
    ) -> Result<PositionResult, String> {
        let mut risk = self.parameters.max_risk_per_trade;
        let account_size = account.balance;

        loop {
            let g6 = risk * account_size;
            let a11 = (entry + (limit1 * 3.0)) / 4.0;
            let a12 = (entry + (limit1 * 3.0) + (limit2 * 5.0)) / 9.0;

            let e8 = if tp > entry { tp - entry } else { entry - tp };
            let d7 = e8 / entry;

            let d8 = if entry > sl {
                g6 / (entry - sl)
            } else {
                g6 / (sl - entry)
            };

            let e11 = if tp > entry {
                let ratio = (d7 / 4.0) * a11;
                a11 + ratio
            } else {
                let ratio = (d7 / 4.0) * a11;
                a11 - ratio
            };

            let e12 = if tp > entry {
                let ratio = (d7 / 6.0) * a12;
                a12 + ratio
            } else {
                let ratio = (d7 / 6.0) * a12;
                a12 - ratio
            };

            let d5 = d8 / 9.0;
            let d11 = d5 * 3.0;
            let d12 = d5 * 5.0;

            let total_position_size = d5 + d11 + d12;
            let max_margin = ((total_position_size * a12).abs()) / ((account_size * leverage) * 0.60);

            if max_margin <= 1.0 {
                return Ok(PositionResult {
                    initial_position_size: d5,
                    limit1_position_size: d11,
                    limit2_position_size: d12,
                    new_tp1: e11,
                    new_tp2: e12,
                    max_margin,
                    final_risk: risk,
                });
            }

            risk -= 0.01;
            if risk <= 0.0 {
                return Err("Unable to calculate a safe risk level under margin limit".to_string());
            }
        }
    }
}
