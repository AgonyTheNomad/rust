// src/strategy/mod.rs

use std::collections::{VecDeque, HashMap};
use serde::{Deserialize, Serialize};
use anyhow::{Result, anyhow};
use log::*;
// Remove unused import
// use chrono::Utc;

// Remove unused imports for Position and PositionStatus
use crate::models::{Candle, PositionType, Signal, Account}; 
// Remove unused import
// use crate::risk::position_calculator::calculate_positions;
use crate::indicators::fibonacci::FibonacciLevels;
use crate::indicators::pivot_points::PivotPoints;
use crate::models::{Position, PositionStatus};
use crate::risk::{RiskManager, RiskParameters};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StrategyConfig {
    pub name: String,
    pub pivot_lookback: usize,
    pub signal_lookback: usize,
    pub fib_threshold: f64,
    pub fib_initial: f64,
    pub fib_tp: f64,
    pub fib_sl: f64,
    pub fib_limit1: f64,
    pub fib_limit2: f64,
    pub min_signal_strength: f64,
    pub initial_balance: f64,
    pub leverage: f64,
    pub max_risk_per_trade: f64,
}

impl Default for StrategyConfig {
    fn default() -> Self {
        Self {
            name: "fibonacci_pivot".to_string(),
            pivot_lookback: 5,
            signal_lookback: 1,
            fib_threshold: 10.0,
            fib_initial: 0.382,
            fib_tp: 0.618,
            fib_sl: 0.236,
            fib_limit1: 0.5,
            fib_limit2: 0.786,
            min_signal_strength: 0.5,
            initial_balance: 10000.0,
            leverage: 20.0,
            max_risk_per_trade: 0.02,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AssetConfig {
    pub name: String,
    pub leverage: f64,
    pub spread: f64,
    pub avg_spread: f64,
}

#[derive(Debug, Clone)]
pub struct PendingOrder {
    pub signal: Signal,
    pub created_at: String,      // Timestamp when order was created
    pub expiry_candles: usize,   // Number of candles after which order expires (0 = never)
    pub candles_active: usize,   // How many candles this order has been active
    pub last_updated: String,    // Timestamp when order was last updated
    pub update_count: usize,     // Number of times this order has been updated
}

pub struct Strategy {
    pub config: StrategyConfig,  // Made public to fix access in backtest
    asset_config: AssetConfig,
    pivot_detector: PivotPoints,
    fib: FibonacciLevels,
    
    // State
    pivot_high_history: VecDeque<Option<f64>>,
    pivot_low_history: VecDeque<Option<f64>>,
    prev_pivot_high: Option<f64>,
    prev_pivot_low: Option<f64>,
    detected_pivot_highs: Vec<f64>,
    detected_pivot_lows: Vec<f64>,
    long_signal: bool,
    short_signal: bool,
    verbose: bool, // Control debug output
    pending_orders: VecDeque<PendingOrder>, // Pending orders waiting for price to hit entry
}

impl Strategy {
    pub fn new(config: StrategyConfig, asset_config: AssetConfig) -> Self {
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
            asset_config,
            pivot_detector: PivotPoints::new(config.pivot_lookback),
            fib,
            pivot_high_history: VecDeque::with_capacity(config.signal_lookback + 2),
            pivot_low_history: VecDeque::with_capacity(config.signal_lookback + 2),
            prev_pivot_high: None,
            prev_pivot_low: None,
            detected_pivot_highs: Vec::new(),
            detected_pivot_lows: Vec::new(),
            long_signal: false,
            short_signal: false,
            verbose: false,
            pending_orders: VecDeque::new(),
        }
    }
    
    // Getter method for config (alternative to making it public)
    pub fn get_max_risk_per_trade(&self) -> f64 {
        self.config.max_risk_per_trade
    }
    
    // Enable or disable verbose logging
    pub fn set_verbose(&mut self, verbose: bool) {
        self.verbose = verbose;
    }
    
    pub fn get_asset_config(&self) -> &AssetConfig {
        &self.asset_config
    }
    
    pub fn initialize_with_history(&mut self, candles: &[Candle]) -> Result<()> {
        if self.verbose {
            debug!("Initializing strategy with {} historical candles", candles.len());
        }
        
        // Process each candle to establish state
        for candle in candles {
            let _ = self.analyze_candle(candle, false)?;
        }
        
        if self.verbose {
            debug!("Strategy initialized with {} pivot highs and {} pivot lows", 
                self.detected_pivot_highs.len(), self.detected_pivot_lows.len());
        }
        
        Ok(())
    }
    
    pub fn analyze_candle(&mut self, candle: &Candle, has_open_position: bool) -> Result<Vec<Signal>> {
        // Reset signal flags from previous runs
        self.long_signal = false;
        self.short_signal = false;
        
        // First, check and process any pending orders
        let triggered_signals = self.check_pending_orders(candle);
        if !triggered_signals.is_empty() {
            return Ok(triggered_signals);
        }
        
        // If a position is already open, don't generate new signals
        if has_open_position {
            return Ok(Vec::new());
        }
        
        // Parse high and low
        let high = candle.high;
        let low = candle.low;
        
        // Detect pivots
        let (pivot_high, pivot_low) = self.pivot_detector.identify_pivots(high, low);
        
        // Update history
        self.pivot_high_history.push_back(pivot_high);
        self.pivot_low_history.push_back(pivot_low);
        
        if self.pivot_high_history.len() > self.config.signal_lookback + 2 {
            self.pivot_high_history.pop_front();
            self.pivot_low_history.pop_front();
        }
        
        // Store detected pivots
        if let Some(high) = pivot_high {
            if self.verbose {
                println!("PIVOT HIGH DETECTED: {:.2} at {}", high, candle.time);
            }
            self.detected_pivot_highs.push(high);
            self.prev_pivot_high = Some(high);
        }
        
        if let Some(low) = pivot_low {
            if self.verbose {
                println!("PIVOT LOW DETECTED: {:.2} at {}", low, candle.time);
            }
            self.detected_pivot_lows.push(low);
            self.prev_pivot_low = Some(low);
        }
        
        // Generate signals
        if self.pivot_high_history.len() >= self.config.signal_lookback + 2 {
            self.generate_signals();
        }
        
        // Also generate from accumulated pivots
        self.generate_signals_from_detected_pivots();
        
        // Create signals and add them to pending orders
        if self.long_signal && self.prev_pivot_high.is_some() && self.prev_pivot_low.is_some() {
            // Add detailed pivot point debugging
            if self.verbose {
                println!("=== LONG SIGNAL PIVOT POINTS ===");
                println!("Pivot High: {:.2}", self.prev_pivot_high.unwrap());
                println!("Pivot Low: {:.2}", self.prev_pivot_low.unwrap());
                println!("Current High: {:.2}, Low: {:.2}", high, low);
                println!("Pivot Range: {:.2}", self.prev_pivot_high.unwrap() - self.prev_pivot_low.unwrap());
            }
            
            // Detect if the signal is valid based on pivot height
            let range = self.prev_pivot_high.unwrap() - self.prev_pivot_low.unwrap();
            if range < self.config.fib_threshold {
                if self.verbose {
                    println!("SIGNAL REJECTED: Range {:.2} is below threshold {:.2}", 
                        range, self.config.fib_threshold);
                }
            } else {
                if self.verbose {
                    println!("SIGNAL VALID: Range {:.2} is above threshold {:.2}",
                        range, self.config.fib_threshold);
                    println!("=============================");
                }
                
                if let Some(levels) = self.calculate_long_levels(
                    self.prev_pivot_high.unwrap(),
                    self.prev_pivot_low.unwrap(),
                ) {
                    let strength = self.calculate_signal_strength(true, high, low);
                    if strength >= self.config.min_signal_strength {
                        let signal = Signal::new(
                            self.asset_config.name.clone(),
                            PositionType::Long,
                            levels.entry_price,
                            levels.take_profit,
                            levels.stop_loss,
                            format!("Pivot high: {}, Pivot low: {}", 
                                self.prev_pivot_high.unwrap(), self.prev_pivot_low.unwrap()),
                            strength,
                        );
                        
                        // Add to pending orders instead of returning directly
                        self.add_pending_order(signal, candle.time.clone(), 50); // 50 candles expiry
                        
                        if self.verbose {
                            debug!("Added PENDING LONG order at {}: Entry={}, TP={}, SL={}, Strength={}",
                                candle.time, levels.entry_price, levels.take_profit, levels.stop_loss, strength);
                        }
                    }
                    self.long_signal = false;
                }
            }
        }
        
        if self.short_signal && self.prev_pivot_high.is_some() && self.prev_pivot_low.is_some() {
            // Add detailed pivot point debugging
            if self.verbose {
                println!("=== SHORT SIGNAL PIVOT POINTS ===");
                println!("Pivot High: {:.2}", self.prev_pivot_high.unwrap());
                println!("Pivot Low: {:.2}", self.prev_pivot_low.unwrap());
                println!("Current High: {:.2}, Low: {:.2}", high, low);
                println!("Pivot Range: {:.2}", self.prev_pivot_high.unwrap() - self.prev_pivot_low.unwrap());
            }
            
            // Detect if the signal is valid based on pivot height
            let range = self.prev_pivot_high.unwrap() - self.prev_pivot_low.unwrap();
            if range < self.config.fib_threshold {
                if self.verbose {
                    println!("SIGNAL REJECTED: Range {:.2} is below threshold {:.2}", 
                        range, self.config.fib_threshold);
                }
            } else {
                if self.verbose {
                    println!("SIGNAL VALID: Range {:.2} is above threshold {:.2}",
                        range, self.config.fib_threshold);
                    println!("=============================");
                }
                
                if let Some(levels) = self.calculate_short_levels(
                    self.prev_pivot_high.unwrap(),
                    self.prev_pivot_low.unwrap(),
                ) {
                    let strength = self.calculate_signal_strength(false, high, low);
                    if strength >= self.config.min_signal_strength {
                        let signal = Signal::new(
                            self.asset_config.name.clone(),
                            PositionType::Short,
                            levels.entry_price,
                            levels.take_profit,
                            levels.stop_loss,
                            format!("Pivot high: {}, Pivot low: {}", 
                                self.prev_pivot_high.unwrap(), self.prev_pivot_low.unwrap()),
                            strength,
                        );
                        
                        // Add to pending orders instead of returning directly
                        self.add_pending_order(signal, candle.time.clone(), 50); // 50 candles expiry
                        
                        if self.verbose {
                            debug!("Added PENDING SHORT order at {}: Entry={}, TP={}, SL={}, Strength={}",
                                candle.time, levels.entry_price, levels.take_profit, levels.stop_loss, strength);
                        }
                    }
                    self.short_signal = false;
                }
            }
        }
        
        // Return an empty vector - no signals are triggered yet
        Ok(Vec::new())
    }
    
    // Modified method to add pending orders with ability to update existing orders
    fn add_pending_order(&mut self, signal: Signal, time: String, expiry_candles: usize) {
        // Check if we have a similar signal already in pending orders
        let similar_order_index = self.find_similar_pending_order(&signal);
        
        if let Some(idx) = similar_order_index {
            // Update the existing order
            let order = &mut self.pending_orders[idx];
            
            // Check if the new signal is better (e.g., better risk-reward)
            let old_risk = (order.signal.price - order.signal.stop_loss).abs();
            let old_reward = (order.signal.take_profit - order.signal.price).abs();
            let old_risk_reward = if old_risk > 0.0 { old_reward / old_risk } else { 0.0 };
            
            let new_risk = (signal.price - signal.stop_loss).abs();
            let new_reward = (signal.take_profit - signal.price).abs();
            let new_risk_reward = if new_risk > 0.0 { new_reward / new_risk } else { 0.0 };
            
            // Only update if the new signal has better risk/reward or higher strength
            if new_risk_reward > old_risk_reward || signal.strength > order.signal.strength {
                if self.verbose {
                    println!("UPDATING PENDING ORDER:");
                    println!("  Type: {}", if matches!(signal.position_type, PositionType::Long) { "LONG" } else { "SHORT" });
                    println!("  Previous Entry: ${:.2}, SL: ${:.2}, TP: ${:.2}, R/R: {:.2}",
                        order.signal.price, order.signal.stop_loss, order.signal.take_profit, old_risk_reward);
                    println!("  New Entry: ${:.2}, SL: ${:.2}, TP: ${:.2}, R/R: {:.2}",
                        signal.price, signal.stop_loss, signal.take_profit, new_risk_reward);
                    println!("  Update count: {}", order.update_count + 1);
                }
                
                // Update the order
                order.signal = signal;
                order.last_updated = time;
                order.update_count += 1;
                
                // Optionally extend the expiry
                order.expiry_candles = expiry_candles.max(order.expiry_candles);
            }
        } else {
            // Maximum number of pending orders to maintain
            let max_pending_orders = 5;
            
            // If we already have too many pending orders, remove the oldest one
            if self.pending_orders.len() >= max_pending_orders {
                self.pending_orders.pop_front();
            }
            
            // Create a new pending order
            let pending = PendingOrder {
                signal,
                created_at: time.clone(),
                expiry_candles,
                candles_active: 0,
                last_updated: time,
                update_count: 0,
            };
            
            if self.verbose {
                println!("NEW PENDING ORDER:");
                println!("  Type: {}", if matches!(pending.signal.position_type, PositionType::Long) { "LONG" } else { "SHORT" });
                println!("  Entry Price: ${:.2}", pending.signal.price);
                println!("  Take Profit: ${:.2}", pending.signal.take_profit);
                println!("  Stop Loss: ${:.2}", pending.signal.stop_loss);
                println!("  Expiry: After {} candles", pending.expiry_candles);
            }
            
            self.pending_orders.push_back(pending);
        }
    }
    
    // Helper method to find similar pending orders
    fn find_similar_pending_order(&self, signal: &Signal) -> Option<usize> {
        // Define what makes orders "similar" - this can be customized
        // Here we consider them similar if they have the same position type 
        // and within 2% price difference
        
        for (i, order) in self.pending_orders.iter().enumerate() {
            // Must be same position type
            if std::mem::discriminant(&order.signal.position_type) != std::mem::discriminant(&signal.position_type) {
                continue;
            }
            
            // Calculate price difference as percentage
            let price_diff_percent = (order.signal.price - signal.price).abs() / order.signal.price;
            
            // If price is within 2% and same position type, consider it similar
            if price_diff_percent < 0.02 {
                return Some(i);
            }
        }
        
        None
    }
    
    // Method to check if any pending orders should be triggered
    fn check_pending_orders(&mut self, candle: &Candle) -> Vec<Signal> {
        let mut triggered = Vec::new();
        let mut to_remove = Vec::new();
        
        for (i, order) in self.pending_orders.iter_mut().enumerate() {
            // Increment active candles counter
            order.candles_active += 1;
            
            // Check if order should expire
            if order.expiry_candles > 0 && order.candles_active >= order.expiry_candles {
                to_remove.push(i);
                if self.verbose {
                    println!("Pending order expired after {} candles", order.candles_active);
                }
                continue;
            }
            
            // Check if price has hit the entry level
            let triggered_price = match order.signal.position_type {
                PositionType::Long => {
                    // For long positions, we enter if price drops to or below our entry price
                    // Make sure the candle actually crosses our entry price
                    if candle.low <= order.signal.price && candle.high >= order.signal.price {
                        Some(order.signal.price)
                    } else {
                        None
                    }
                },
                PositionType::Short => {
                    // For short positions, we enter if price rises to or above our entry price
                    // Make sure the candle actually crosses our entry price
                    if candle.high >= order.signal.price && candle.low <= order.signal.price {
                        Some(order.signal.price)
                    } else {
                        None
                    }
                },
            };
            
            // Fix unused variable warning by using underscore prefix
            if let Some(_price) = triggered_price {
                // Order triggered - modify the signal to indicate it's been triggered
                let mut triggered_signal = order.signal.clone();
                // Fixed: Now using Some() to wrap the String value
                triggered_signal.status = Some("Triggered".to_string());
                
                triggered.push(triggered_signal);
                to_remove.push(i);
                
                if self.verbose {
                    println!("PENDING ORDER TRIGGERED:");
                    println!("  Type: {}", if matches!(order.signal.position_type, PositionType::Long) { "LONG" } else { "SHORT" });
                    println!("  Entry Price: ${:.2}", order.signal.price);
                    println!("  Candle High: ${:.2}, Low: ${:.2}", candle.high, candle.low);
                    println!("  After waiting {} candles", order.candles_active);
                    println!("  Update count: {}", order.update_count);
                    println!("  Candle time: {}", candle.time);
                }
            }
        }
        
        // Remove triggered/expired orders (in reverse to maintain correct indices)
        for idx in to_remove.iter().rev() {
            self.pending_orders.remove(*idx);
        }
        
        triggered
    }
    
    // Add a method to get information about pending orders
    pub fn get_pending_orders_info(&self) -> Vec<String> {
        self.pending_orders.iter()
            .map(|order| {
                format!(
                    "{} order @ ${:.2} (Stop: ${:.2}, TP: ${:.2}), active for {} candles, updated {} times",
                    if matches!(order.signal.position_type, PositionType::Long) { "LONG" } else { "SHORT" },
                    order.signal.price,
                    order.signal.stop_loss,
                    order.signal.take_profit,
                    order.candles_active,
                    order.update_count
                )
            })
            .collect()
    }
    
    // Improved implementation of level calculation with proper limit order placement
    fn calculate_long_levels(&self, prev_high: f64, prev_low: f64) -> Option<crate::indicators::fibonacci::FibLevels> {
        let range = prev_high - prev_low;
        if range < self.config.fib_threshold {
            return None;
        }
    
        // Calculate the main price levels
        let entry_price = prev_low + self.config.fib_initial * range;
        let take_profit = prev_low + self.config.fib_tp * range; // Use Fibonacci extension
        let stop_loss = prev_low - self.config.fib_sl * range; // Set stop loss below the low
        
        // Calculate limit prices directly from pivot points
        let limit1 = prev_low + self.config.fib_limit1 * range;
        let limit2 = prev_low + self.config.fib_limit2 * range;
        
        if self.verbose {
            println!("LONG POSITION LEVELS (USING PIVOT POINTS):");
            println!("  Price Range: {:.2} (from {:.2} to {:.2})", range, prev_low, prev_high);
            println!("  Entry: {:.2}", entry_price);
            println!("  Take Profit: {:.2}", take_profit);
            println!("  Stop Loss: {:.2}", stop_loss);
            println!("  Limit1: {:.2}", limit1);
            println!("  Limit2: {:.2}", limit2);
            
            // Validate the order of price levels
            if !(stop_loss < limit2 && limit2 < limit1 && limit1 < entry_price && entry_price < take_profit) {
                println!("WARNING: Invalid price order for long position!");
            }
        }
    
        Some(crate::indicators::fibonacci::FibLevels {
            entry_price,
            take_profit,
            stop_loss,
            limit1,
            limit2,
        })
    }
    
    fn calculate_short_levels(&self, prev_high: f64, prev_low: f64) -> Option<crate::indicators::fibonacci::FibLevels> {
        let range = prev_high - prev_low;
        if range < self.config.fib_threshold {
            return None;
        }
    
        // Calculate the main price levels
        let entry_price = prev_high - self.config.fib_initial * range;
        let take_profit = prev_high - self.config.fib_tp * range; // Use Fibonacci extension downward
        let stop_loss = prev_high + self.config.fib_sl * range; // Set stop loss above the high
        
        // Calculate limit prices directly from pivot points
        let limit1 = prev_high - self.config.fib_limit1 * range; 
        let limit2 = prev_high - self.config.fib_limit2 * range;
        
        if self.verbose {
            println!("SHORT POSITION LEVELS (USING PIVOT POINTS):");
            println!("  Price Range: {:.2} (from {:.2} to {:.2})", range, prev_low, prev_high);
            println!("  Entry: {:.2}", entry_price);
            println!("  Take Profit: {:.2}", take_profit);
            println!("  Stop Loss: {:.2}", stop_loss);
            println!("  Limit1: {:.2}", limit1);
            println!("  Limit2: {:.2}", limit2);
            
            // Validate the order of price levels
            // instead of stop > limit2 > limit1 > entry > tp
            if !(stop_loss > limit1
                && limit1    > limit2
                && limit2    > entry_price
                && entry_price > take_profit)
            {
                println!("WARNING: Invalid price order for short position!");
            }
 
        }
    
        Some(crate::indicators::fibonacci::FibLevels {
            entry_price,
            take_profit,
            stop_loss,
            limit1,
            limit2,
        })
    }
    
    // Here's the fixed create_scaled_position method
    pub fn create_scaled_position(
        &self,
        signal: &Signal,
        account_size: f64,
        risk: f64,
    ) -> Result<Position> {
        // 0) Debug: print signal details
        if self.verbose {
            println!("POSITION CREATION FROM SIGNAL");
            println!("Signal Details:");
            println!(
                "  Type: {}",
                if matches!(signal.position_type, PositionType::Long) {
                    "LONG"
                } else {
                    "SHORT"
                }
            );
            println!("  Entry:       {:.2}", signal.price);
            println!("  Take Profit: {:.2}", signal.take_profit);
            println!("  Stop Loss:   {:.2}", signal.stop_loss);
            println!("  Reason:      {}", signal.reason);
            println!("  Strength:    {:.2}", signal.strength);
            if let Some(status) = &signal.status {
                println!("  Status:      {}", status);
            }
        }

        // 1) Extract pivots from signal.reason ("Pivot high: XXX, Pivot low: YYY")
        let pivots = signal
            .reason
            .split(',')
            .map(str::trim)
            .collect::<Vec<_>>();
        let (prev_high, prev_low) = if pivots.len() == 2 {
            let high = pivots[0]
                .split_once(':')
                .and_then(|(_, v)| v.trim().parse::<f64>().ok())
                .unwrap_or(0.0);
            let low = pivots[1]
                .split_once(':')
                .and_then(|(_, v)| v.trim().parse::<f64>().ok())
                .unwrap_or(0.0);
            if self.verbose {
                println!("Extracted pivot values - High: {:.2}, Low: {:.2}", high, low);
            }
            (high, low)
        } else {
            if self.verbose {
                println!("WARNING: Could not parse pivots from reason: {}", signal.reason);
            }
            (0.0, 0.0)
        };

        // 2) Compute pivot-range, reject too-small moves
        let range = prev_high - prev_low;
        if range < self.config.fib_threshold {
            if self.verbose {
                println!(
                    "SIGNAL REJECTED: Range {:.2} < threshold {:.2}",
                    range, self.config.fib_threshold
                );
            }
            return Err(anyhow!("Pivot range below threshold"));
        }

        // 3) Main levels (all based on pivot-range)
        let entry_price = match signal.position_type {
            PositionType::Long => prev_low + self.config.fib_initial * range,
            PositionType::Short => prev_high - self.config.fib_initial * range,
        };
        let take_profit = match signal.position_type {
            PositionType::Long => prev_high + self.config.fib_tp * range,
            PositionType::Short => prev_low - self.config.fib_tp * range,
        };
        let stop_loss = match signal.position_type {
            PositionType::Long => prev_low - self.config.fib_sl * range,
            PositionType::Short => prev_high + self.config.fib_sl * range,
        };

        // 4) Pivot-range based limits:
        //    For a SHORT we want limit2 (farthest) = prev_high - fib_limit1 * range,
        //        and limit1 (closer) = prev_high - fib_limit2 * range.
        //    For a LONG we do the mirror: limit2 = prev_low + fib_limit1 * range, etc.
        let (limit1_price, limit2_price) = match signal.position_type {
            PositionType::Long => {
                let l2 = prev_low + self.config.fib_limit1 * range;
                let l1 = prev_low + self.config.fib_limit2 * range;
                (l1, l2)
            }
            PositionType::Short => {
                let l2 = prev_high - self.config.fib_limit1 * range;
                let l1 = prev_high - self.config.fib_limit2 * range;
                (l1, l2)
            }
        };

        // 5) Verbose debug print of all levels
        if self.verbose {
            println!(
                "{} POSITION LEVELS (USING PIVOT POINTS):",
                if matches!(signal.position_type, PositionType::Long) {
                    "LONG"
                } else {
                    "SHORT"
                }
            );
            println!("  Pivot High: {:.2}", prev_high);
            println!("  Pivot Low:  {:.2}", prev_low);
            println!("  Price Range: {:.2}", range);
            println!("  Entry:       {:.2}", entry_price);
            println!("  Take Profit: {:.2}", take_profit);
            println!("  Stop Loss:   {:.2}", stop_loss);
            println!("  Limit1:      {:.2}", limit1_price);
            println!("  Limit2:      {:.2}", limit2_price);

            // enforce correct ordering
            let ok = if matches!(signal.position_type, PositionType::Long) {
                stop_loss < limit2_price
                    && limit2_price < limit1_price
                    && limit1_price < entry_price
                    && entry_price < take_profit
            } else {
                stop_loss > limit2_price
                    && limit2_price > limit1_price
                    && limit1_price > entry_price
                    && entry_price > take_profit
            };
            if !ok {
                println!("WARNING: Invalid price order for {:?} position!", signal.position_type);
            }
        }

        // 6) Size the position & get limit sizes
        let risk_params = RiskParameters {
            max_risk_per_trade: risk,
            max_position_size: 10.0,
            max_leverage: self.asset_config.leverage,
            spread: self.asset_config.spread,
        };
        
        let risk_manager = RiskManager::new(risk_params);
        
        // Create an account for position sizing
        let account = Account {
            balance: account_size,
            equity: account_size,
            used_margin: 0.0,
            positions: HashMap::new(),
        };
        
        // Calculate position scale using risk manager
        let sizing = risk_manager.calculate_positions_with_risk(
            &account,
            entry_price,
            take_profit,
            stop_loss,
            limit1_price,
            limit2_price,
            self.asset_config.leverage,
            signal.position_type.clone(),
        )?;

        // 7) Construct the Position
        let position = Position {
            id: uuid::Uuid::new_v4().to_string(),
            symbol: signal.symbol.clone(),
            entry_time: chrono::Utc::now().to_string(),
            entry_price,
            size: sizing.initial_position_size,
            stop_loss,
            take_profit,
            position_type: signal.position_type.clone(),
            risk_percent: sizing.final_risk,
            margin_used: (sizing.initial_position_size * entry_price) / self.asset_config.leverage,
            status: if signal.status.as_deref() == Some("Triggered") {
                PositionStatus::Triggered
            } else {
                PositionStatus::Pending
            },
            limit1_price: Some(limit1_price),
            limit2_price: Some(limit2_price),
            limit1_hit: false,
            limit2_hit: false,
            limit1_size: sizing.limit1_position_size,
            limit2_size: sizing.limit2_position_size,
            new_tp1: Some(sizing.new_tp1),
            new_tp2: Some(sizing.new_tp2),
            entry_order_id: None,
            tp_order_id: None,
            sl_order_id: None,
            limit1_order_id: None,
            limit2_order_id: None,
            limit1_time: None,
            limit2_time: None,
            new_tp: None,
        };

        if self.verbose {
            println!("FINAL POSITION VALIDATION: {:?}", position);
        }

        Ok(position)
    }
    
    fn generate_signals(&mut self) {
        let prev_idx = 0;
        let curr_idx = 1 + self.config.signal_lookback;
        
        let prev_pivot_high = self.pivot_high_history[prev_idx];
        let curr_pivot_high = self.pivot_high_history[curr_idx];
        
        let prev_pivot_low = self.pivot_low_history[prev_idx];
        let curr_pivot_low = self.pivot_low_history[curr_idx];
        
        if let (Some(prev), Some(curr)) = (prev_pivot_high, curr_pivot_high) {
            if curr > prev {
                if self.verbose {
                    println!("SIGNAL GENERATION: Higher pivot high detected ({:.2} > {:.2})", curr, prev);
                }
                self.long_signal = true;
            }
        }
        
        if let (Some(prev), Some(curr)) = (prev_pivot_low, curr_pivot_low) {
            if curr < prev {
                if self.verbose {
                    println!("SIGNAL GENERATION: Lower pivot low detected ({:.2} < {:.2})", curr, prev);
                }
                self.short_signal = true;
            }
        }
    }
    
    fn generate_signals_from_detected_pivots(&mut self) {
        if self.detected_pivot_highs.len() >= 2 {
            let latest = self.detected_pivot_highs[self.detected_pivot_highs.len() - 1];
            let previous = self.detected_pivot_highs[self.detected_pivot_highs.len() - 2];
            if latest > previous {
                if self.verbose {
                    println!("SIGNAL GENERATION: Higher detected pivot high ({:.2} > {:.2})", latest, previous);
                }
                self.long_signal = true;
            }
        }
        
        if self.detected_pivot_lows.len() >= 2 {
            let latest = self.detected_pivot_lows[self.detected_pivot_lows.len() - 1];
            let previous = self.detected_pivot_lows[self.detected_pivot_lows.len() - 2];
            if latest < previous {
                if self.verbose {
                    println!("SIGNAL GENERATION: Lower detected pivot low ({:.2} < {:.2})", latest, previous);
                }
                self.short_signal = true;
            }
        }
    }
    
    fn calculate_signal_strength(&self, is_long: bool, current_high: f64, current_low: f64) -> f64 {
        // Base strength
        let mut strength = 0.7;
        
        // Check if we have enough pivot history
        if self.detected_pivot_highs.len() < 2 || self.detected_pivot_lows.len() < 2 {
            return strength;
        }
        
        let len_highs = self.detected_pivot_highs.len();
        let len_lows = self.detected_pivot_lows.len();
        
        // Calculate trendiness
        let trend_strength = if is_long {
            // For long signals, check if we have consecutive higher highs and higher lows
            let higher_highs = if len_highs >= 3 {
                self.detected_pivot_highs[len_highs - 1] > self.detected_pivot_highs[len_highs - 2] &&
                self.detected_pivot_highs[len_highs - 2] > self.detected_pivot_highs[len_highs - 3]
            } else {
                false
            };
            
            let higher_lows = if len_lows >= 2 {
                self.detected_pivot_lows[len_lows - 1] > self.detected_pivot_lows[len_lows - 2]
            } else {
                false
            };
            
            if higher_highs && higher_lows {
                0.2
            } else if higher_highs || higher_lows {
                0.1
            } else {
                0.0
            }
        } else {
            // For short signals, check if we have consecutive lower highs and lower lows
            let lower_highs = if len_highs >= 2 {
                self.detected_pivot_highs[len_highs - 1] < self.detected_pivot_highs[len_highs - 2]
            } else {
                false
            };
            
            let lower_lows = if len_lows >= 3 {
                self.detected_pivot_lows[len_lows - 1] < self.detected_pivot_lows[len_lows - 2] &&
                self.detected_pivot_lows[len_lows - 2] < self.detected_pivot_lows[len_lows - 3]
            } else {
                false
            };
            
            if lower_highs && lower_lows {
                0.2
            } else if lower_highs || lower_lows {
                0.1
            } else {
                0.0
            }
        };
        
        // Adjust strength based on trend
        strength += trend_strength;
        
        // Adjust based on current price position
        if is_long {
            // For long, better if price is closer to low (buying at support)
            if let (Some(last_high), Some(last_low)) = (self.prev_pivot_high, self.prev_pivot_low) {
                let range = last_high - last_low;
                if range > 0.0 {
                    let position = (current_high - last_low) / range;
                    // Prefer if current price is in lower half of range
                    if position < 0.3 {
                        strength += 0.15;
                    } else if position < 0.5 {
                        strength += 0.1;
                    }
                }
            }
        } else {
            // For short, better if price is closer to high (selling at resistance)
            if let (Some(last_high), Some(last_low)) = (self.prev_pivot_high, self.prev_pivot_low) {
                let range = last_high - last_low;
                if range > 0.0 {
                    let position = (last_high - current_low) / range;
                    // Prefer if current price is in upper half of range
                    if position < 0.3 {
                        strength += 0.15;
                    } else if position < 0.5 {
                        strength += 0.1;
                    }
                }
            }
        }
        
        // Print signal strength calculation details
        if self.verbose {
            println!("SIGNAL STRENGTH CALCULATION ({}):", if is_long { "LONG" } else { "SHORT" });
            println!("  Base Strength: 0.7");
            println!("  Trend Strength: {:.2}", trend_strength);
            println!("  Current High: {:.2}, Low: {:.2}", current_high, current_low);
            println!("  Final Strength: {:.2}", strength.max(0.0).min(1.0));
        }
        
        // Ensure strength is between 0 and 1
        strength.max(0.0).min(1.0)
    }
}