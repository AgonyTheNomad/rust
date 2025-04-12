use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};

use crate::models::{Account, Candle, Position, PositionType, Trade, BacktestState};
use crate::risk::{RiskManager, RiskParameters};
use crate::indicators::{PivotPoints, FibonacciLevels};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetConfig {
    pub name: String,
    pub leverage: f64,
    pub spread: f64,
    pub avg_spread: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyConfig {
    pub initial_balance: f64,
    pub leverage: f64,
    pub max_risk_per_trade: f64,
    pub pivot_lookback: usize,
    pub signal_lookback: usize,
    pub fib_threshold: f64,
    pub fib_initial: f64,
    pub fib_tp: f64,
    pub fib_sl: f64,
    pub fib_limit1: f64,
    pub fib_limit2: f64,
}

impl Default for StrategyConfig {
    fn default() -> Self {
        Self {
            initial_balance: 10_000.0,
            leverage: 20.0,
            max_risk_per_trade: 0.02,
            pivot_lookback: 5,
            signal_lookback: 1,
            fib_threshold: 10.0,
            fib_initial: 0.236,
            fib_tp: 0.618,
            fib_sl: 0.236,
            fib_limit1: 0.382,
            fib_limit2: 0.5,
        }
    }
}

pub struct Strategy {
    config: StrategyConfig,
    risk_manager: RiskManager,
    pivot_detector: PivotPoints,
    fib: FibonacciLevels,
    pivot_high_history: VecDeque<Option<f64>>,
    pivot_low_history: VecDeque<Option<f64>>,
    prev_pivot_high: Option<f64>,
    prev_pivot_low: Option<f64>,
    long_signal: bool,
    short_signal: bool,
    // New fields to track actual detected pivots
    detected_pivot_highs: Vec<f64>,
    detected_pivot_lows: Vec<f64>,
}

impl Strategy {
    pub fn new(config: StrategyConfig, asset_config: AssetConfig) -> Self {
        let risk_parameters = RiskParameters {
            max_risk_per_trade: config.max_risk_per_trade,
            max_position_size: 10.0,
            max_leverage: config.leverage,
            spread: asset_config.spread,
        };

        let fib = FibonacciLevels::new(
            config.fib_threshold,
            config.fib_initial,
            config.fib_tp,
            config.fib_sl,
            config.fib_limit1,
            config.fib_limit2,
        );

        Self {
            config: config.clone(),
            risk_manager: RiskManager::new(risk_parameters),
            pivot_detector: PivotPoints::new(config.pivot_lookback),
            fib,
            pivot_high_history: VecDeque::with_capacity(config.signal_lookback + 2),
            pivot_low_history: VecDeque::with_capacity(config.signal_lookback + 2),
            prev_pivot_high: None,
            prev_pivot_low: None,
            long_signal: false,
            short_signal: false,
            // Initialize the new fields
            detected_pivot_highs: Vec::new(),
            detected_pivot_lows: Vec::new(),
        }
    }

    pub fn analyze_candle(
        &mut self,
        current_candle: &Candle,
        state: &mut BacktestState,
    ) -> Option<Trade> {
        let (pivot_high, pivot_low) = self.pivot_detector.identify_pivots(
            current_candle.high,
            current_candle.low,
        );

        // Keep the existing history logic for compatibility
        self.pivot_high_history.push_back(pivot_high);
        self.pivot_low_history.push_back(pivot_low);

        if self.pivot_high_history.len() > self.config.signal_lookback + 2 {
            self.pivot_high_history.pop_front();
            self.pivot_low_history.pop_front();
        }

        // Add newly detected pivots to our vectors of actual detected pivots
        if let Some(high) = pivot_high {
            self.detected_pivot_highs.push(high);
            self.prev_pivot_high = Some(high);
        }

        if let Some(low) = pivot_low {
            self.detected_pivot_lows.push(low);
            self.prev_pivot_low = Some(low);
        }

        // Use the original method for compatibility but also check with the new method
        if self.pivot_high_history.len() >= self.config.signal_lookback + 2 {
            self.generate_signals();
        }

        // Also generate signals using the new approach
        self.generate_signals_from_detected_pivots();

        // Check for exits if we have an open position
        if let Some(position) = &mut state.position {
            if let Some(trade) = self.check_exits(current_candle, position) {
                state.position = None;
                state.account_balance += trade.pnl;
                state.trades.push(trade.clone());

                if state.account_balance > state.peak_balance {
                    state.peak_balance = state.account_balance;
                } else {
                    let drawdown = (state.peak_balance - state.account_balance) / state.peak_balance;
                    state.current_drawdown = drawdown;
                    if drawdown > state.max_drawdown {
                        state.max_drawdown = drawdown;
                    }
                }

                return Some(trade);
            }
        }

        // Enter positions if we have signals and no current position
        if state.position.is_none() {
            // FIX: Check if both prev_pivot_high and prev_pivot_low are Some values before unwrapping
            if self.long_signal && self.prev_pivot_high.is_some() && self.prev_pivot_low.is_some() {
                if let Some(levels) = self.fib.calculate_long_levels(
                    self.prev_pivot_high.unwrap(),
                    self.prev_pivot_low.unwrap(),
                ) {
                    self.enter_position(current_candle, state, PositionType::Long, levels);
                    self.long_signal = false;
                }
            } else if self.short_signal && self.prev_pivot_high.is_some() && self.prev_pivot_low.is_some() {
                if let Some(levels) = self.fib.calculate_short_levels(
                    self.prev_pivot_high.unwrap(),
                    self.prev_pivot_low.unwrap(),
                ) {
                    self.enter_position(current_candle, state, PositionType::Short, levels);
                    self.short_signal = false;
                }
            }
        }

        None
    }

    fn enter_position(
        &mut self,
        candle: &Candle,
        state: &mut BacktestState,
        position_type: PositionType,
        levels: crate::indicators::FibLevels,
    ) {
        let account = Account {
            balance: state.account_balance,
            equity: state.account_balance,
            used_margin: 0.0,
            positions: HashMap::new(),
        };
    
        // Pass the position type to calculate_positions_with_risk
        let result = self.risk_manager.calculate_positions_with_risk(
            &account,
            levels.entry_price,
            levels.take_profit,
            levels.stop_loss,
            levels.limit1,
            levels.limit2,
            self.config.leverage,
            position_type.clone(), // Pass position_type
        );
    
        if let Ok(position_result) = result {
            state.position = Some(Position {
                entry_time: candle.time.clone(),
                entry_price: levels.entry_price,
                size: position_result.initial_position_size,
                stop_loss: levels.stop_loss,
                take_profit: levels.take_profit,
                position_type,
                risk_percent: position_result.final_risk,
                margin_used: position_result.max_margin,
    
                // Scaling fields
                limit1_price: Some(levels.limit1),
                limit2_price: Some(levels.limit2),
                limit1_hit: false,
                limit2_hit: false,
                limit1_size: position_result.limit1_position_size,
                limit2_size: position_result.limit2_position_size,
                new_tp1: Some(position_result.new_tp1),
                new_tp2: Some(position_result.new_tp2),
            });
        }
    }
    
    fn check_exits(&self, candle: &Candle, position: &mut Position) -> Option<Trade> {
        // Check Limit 1
        if !position.limit1_hit {
            if let Some(limit1) = position.limit1_price {
                let hit = match position.position_type {
                    PositionType::Long => candle.low <= limit1,
                    PositionType::Short => candle.high >= limit1,
                };
                if hit {
                    // Store old values for logging
                    let old_take_profit = position.take_profit;
                    
                    position.size += position.limit1_size;
                    position.take_profit = position.new_tp1.unwrap_or(position.take_profit);
                    
                    // No change to stop loss - removed the code that would adjust it
                    
                    position.limit1_hit = true;
                    
                    // Optional logging - only mentioning take profit changes
                    println!(
                        "Limit1 hit at {}: TP changed from ${:.2} to ${:.2}", 
                        candle.time, old_take_profit, position.take_profit
                    );
                }
            }
        }
    
        // Check Limit 2
        if !position.limit2_hit {
            if let Some(limit2) = position.limit2_price {
                let hit = match position.position_type {
                    PositionType::Long => candle.low <= limit2,
                    PositionType::Short => candle.high >= limit2,
                };
                if hit {
                    // Store old values for logging
                    let old_take_profit = position.take_profit;
                    
                    position.size += position.limit2_size;
                    position.take_profit = position.new_tp2.unwrap_or(position.take_profit);
                    
                    // No change to stop loss - removed the code that would adjust it
                    
                    position.limit2_hit = true;
                    
                    // Optional logging - only mentioning take profit changes
                    println!(
                        "Limit2 hit at {}: TP changed from ${:.2} to ${:.2}", 
                        candle.time, old_take_profit, position.take_profit
                    );
                }
            }
        }
    
        // Check TP or SL
        let (exit_price, should_exit, exit_type) = match position.position_type {
            PositionType::Long => {
                if candle.low <= position.stop_loss {
                    (position.stop_loss, true, "STOP LOSS")
                } else if candle.high >= position.take_profit {
                    (position.take_profit, true, "TAKE PROFIT")
                } else {
                    (0.0, false, "")
                }
            }
            PositionType::Short => {
                if candle.high >= position.stop_loss {
                    (position.stop_loss, true, "STOP LOSS")
                } else if candle.low <= position.take_profit {
                    (position.take_profit, true, "TAKE PROFIT")
                } else {
                    (0.0, false, "")
                }
            }
        };
    
        if should_exit {
            // Log the exit type
            println!("Position exit at {}: {} triggered!", candle.time, exit_type);
            
            let pnl = match position.position_type {
                PositionType::Long => (exit_price - position.entry_price) * position.size,
                PositionType::Short => (position.entry_price - exit_price) * position.size,
            };
    
            let profit_factor = if pnl > 0.0 {
                pnl / (position.size * position.entry_price)
            } else {
                0.0
            };
    
            return Some(Trade {
                entry_time: position.entry_time.clone(),
                exit_time: candle.time.clone(),
                position_type: format!("{:?}", position.position_type),
                entry_price: position.entry_price,
                exit_price,
                size: position.size,
                pnl,
                risk_percent: position.risk_percent,
                profit_factor,
                margin_used: position.margin_used,
                fees: 0.0,         // Initialize with zero, will be updated in execute_trade
                slippage: 0.0,     // Initialize with zero, will be updated in execute_trade
            });
        }
    
        None
    }

    // Original method kept for compatibility
    fn generate_signals(&mut self) {
        let prev_idx = 0;
        let curr_idx = 1 + self.config.signal_lookback;

        let prev_pivot_high = self.pivot_high_history[prev_idx];
        let curr_pivot_high = self.pivot_high_history[curr_idx];

        let prev_pivot_low = self.pivot_low_history[prev_idx];
        let curr_pivot_low = self.pivot_low_history[curr_idx];

        // Long signal: current pivot high > previous pivot high
        if let (Some(prev), Some(curr)) = (prev_pivot_high, curr_pivot_high) {
            if curr > prev {
                self.long_signal = true;
            }
        }

        // Short signal: current pivot low < previous pivot low
        if let (Some(prev), Some(curr)) = (prev_pivot_low, curr_pivot_low) {
            if curr < prev {
                self.short_signal = true;
            }
        }
    }

    // New method that directly compares detected pivot points
    fn generate_signals_from_detected_pivots(&mut self) {
        // Check for higher high pattern (long signal)
        if self.detected_pivot_highs.len() >= 2 {
            let latest = self.detected_pivot_highs[self.detected_pivot_highs.len() - 1];
            let previous = self.detected_pivot_highs[self.detected_pivot_highs.len() - 2];
            
            if latest > previous {
                self.long_signal = true;
            }
        }
        
        // Check for lower low pattern (short signal)
        if self.detected_pivot_lows.len() >= 2 {
            let latest = self.detected_pivot_lows[self.detected_pivot_lows.len() - 1];
            let previous = self.detected_pivot_lows[self.detected_pivot_lows.len() - 2];
            
            if latest < previous {
                self.short_signal = true;
            }
        }
    }

    // Accessor methods for testing
    pub fn is_long_signal(&self) -> bool {
        self.long_signal
    }

    pub fn is_short_signal(&self) -> bool {
        self.short_signal
    }
}