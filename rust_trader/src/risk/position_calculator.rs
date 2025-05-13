// src/risk/position_calculator.rs
use crate::models::PositionType;
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionCalculator {
    pub scale_limit1: f64, // Scale factor for limit1 price
    pub scale_limit2: f64, // Scale factor for limit2 price
    pub size_ratio1: f64,  // Size ratio for limit1 order
    pub size_ratio2: f64,  // Size ratio for limit2 order
    pub tp_ratio1: f64,    // TP adjustment ratio after limit1
    pub tp_ratio2: f64,    // TP adjustment ratio after limit2
}

impl Default for PositionCalculator {
    fn default() -> Self {
        Self {
            scale_limit1: 3.0,
            scale_limit2: 5.0,
            size_ratio1: 3.0,
            size_ratio2: 5.0,
            tp_ratio1: 4.0,
            tp_ratio2: 6.0,
        }
    }
}

impl PositionCalculator {
    pub fn new() -> Self {
        Self::default()
    }
    
    // New method to calculate position scaling
    pub fn calculate_position_scaling(
        &self,
        initial: f64,
        tp: f64,
        sl: f64,
        limit_1: f64,
        limit_2: f64,
        account_size: f64,
        risk: f64,
        leverage: f64,
        position_type: PositionType,
    ) -> Result<PositionScaleResult> {
        calculate_positions(
            initial,
            tp,
            sl,
            limit_1,
            limit_2,
            account_size,
            risk,
            leverage,
            position_type,
            self.tp_ratio1,  // Using the struct's tp_ratio1 instead of hardcoded h11
            self.tp_ratio2,  // Using the struct's tp_ratio2 instead of hardcoded h12
        )
    }
}

#[derive(Debug, Clone)]
pub struct PositionResult {
    pub size: f64,
    pub risk_amount: f64,
    pub margin_required: f64,
}

pub fn calculate_positions(
    initial: f64,
    tp: f64,
    sl: f64,
    limit_1: f64,
    limit_2: f64,
    account_size: f64,
    risk: f64,
    leverage: f64,
    position_type: PositionType,
    h11: f64, // Default value of 4.0
    h12: f64, // Default value of 6.0
) -> Result<PositionScaleResult> {
    let mut current_risk = risk;

    loop {
        let g6 = current_risk * account_size;
        let a11 = (initial + (limit_1 * 3.0)) / 4.0;
        let a12 = (initial + (limit_1 * 3.0) + (limit_2 * 5.0)) / 9.0;

        // Calculate e8 (difference between TP and entry price)
        let e8 = match position_type {
            PositionType::Long => tp - initial,
            PositionType::Short => initial - tp,
        };
        
        let d7 = e8 / initial;
        
        // Calculate d8 using a12 instead of initial
        let d8 = match position_type {
            PositionType::Long => g6 / (a12 - sl),
            PositionType::Short => g6 / (sl - a12),
        };
        
        // Calculate e11 (new take profit 1)
        let e11 = match position_type {
            PositionType::Long => {
                let ratio = (d7 / h11) * a11;
                a11 + ratio
            },
            PositionType::Short => {
                let ratio = (d7 / h11) * a11;
                a11 - ratio
            },
        };
        
        // Calculate e12 (new take profit 2)
        let e12 = match position_type {
            PositionType::Long => {
                let ratio = (d7 / h12) * a12;
                a12 + ratio
            },
            PositionType::Short => {
                let ratio = (d7 / h12) * a12;
                a12 - ratio
            },
        };
        
        // Position sizes for initial, limit1, and limit2
        let d5 = d8 / 9.0;          // Initial position size
        let d11 = d5 * 3.0;         // Limit 1 position size
        let d12 = d5 * 5.0;         // Limit 2 position size
        
        // Calculate max margin
        let total_position_size = d5 + d11 + d12;
        let max_margin = ((total_position_size * a12).abs()) / ((account_size * leverage) * 0.60);
        
        // If margin is acceptable, return the result
        if max_margin <= 1.0 {
            return Ok(PositionScaleResult {
                initial_position_size: d5,
                limit1_position_size: d11,
                limit2_position_size: d12,
                new_tp1: e11,
                new_tp2: e12,
                max_margin,
                final_risk: current_risk,
            });
        }
        
        // Reduce risk and try again
        current_risk -= 0.01;
        
        // Safety check to prevent infinite loop
        if current_risk <= 0.0 {
            return Err(anyhow::anyhow!("Unable to calculate a safe risk level under margin limit"));
        }
    }
}

#[derive(Debug, Clone)]
pub struct PositionScaleResult {
    pub initial_position_size: f64,
    pub limit1_position_size: f64,
    pub limit2_position_size: f64,
    pub new_tp1: f64,
    pub new_tp2: f64,
    pub max_margin: f64,
    pub final_risk: f64,
}