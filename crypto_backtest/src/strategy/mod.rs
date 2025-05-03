// src/strategy/mod.rs

use std::collections::VecDeque;
use serde::{Deserialize, Serialize};
use anyhow::{Result};
use log::*;
use chrono::Utc;

use crate::models::{Candle, PositionType, Signal, Position, PositionStatus};
use crate::risk::position_calculator::calculate_positions;
use crate::indicators::fibonacci::FibonacciLevels;
use crate::indicators::pivot_points::PivotPoints;

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

pub struct Strategy {
    config: StrategyConfig,
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
        }
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
        
        // If a position is already open, return empty signals vector
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
        
        // Create and return signals if generated
        let mut signals = Vec::new();
        
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
                        
                        signals.push(signal);
                        if self.verbose {
                            debug!("Generated LONG signal at {}: Entry={}, TP={}, SL={}, Strength={}",
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
                        
                        signals.push(signal);
                        if self.verbose {
                            debug!("Generated SHORT signal at {}: Entry={}, TP={}, SL={}, Strength={}",
                                candle.time, levels.entry_price, levels.take_profit, levels.stop_loss, strength);
                        }
                    }
                    self.short_signal = false;
                }
            }
        }
        
        Ok(signals)
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
        
        // Calculate limit prices relative to entry and stop loss
        let entry_to_sl_range = entry_price - stop_loss;
        let limit1 = entry_price - (self.config.fib_limit1 * entry_to_sl_range);
        let limit2 = entry_price - (self.config.fib_limit2 * entry_to_sl_range);
        
        if self.verbose {
            println!("LONG POSITION LEVELS:");
            println!("  Price Range: {:.2} (from {:.2} to {:.2})", range, prev_low, prev_high);
            println!("  Entry: {:.2}", entry_price);
            println!("  Take Profit: {:.2}", take_profit);
            println!("  Stop Loss: {:.2}", stop_loss);
            println!("  Entry to SL Range: {:.2}", entry_to_sl_range);
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
        
        // Calculate limit prices relative to entry and stop loss
        let entry_to_sl_range = stop_loss - entry_price;
        let limit1 = entry_price + (self.config.fib_limit1 * entry_to_sl_range);
        let limit2 = entry_price + (self.config.fib_limit2 * entry_to_sl_range);
        
        if self.verbose {
            println!("SHORT POSITION LEVELS:");
            println!("  Price Range: {:.2} (from {:.2} to {:.2})", range, prev_low, prev_high);
            println!("  Entry: {:.2}", entry_price);
            println!("  Take Profit: {:.2}", take_profit);
            println!("  Stop Loss: {:.2}", stop_loss);
            println!("  Entry to SL Range: {:.2}", entry_to_sl_range);
            println!("  Limit1: {:.2}", limit1);
            println!("  Limit2: {:.2}", limit2);
            
            // Validate the order of price levels
            if !(stop_loss > limit2 && limit2 > limit1 && limit1 > entry_price && entry_price > take_profit) {
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
    
    // Method to create a scaled position from a signal
    pub fn create_scaled_position(&self, signal: &Signal, account_size: f64, risk: f64) -> Result<Position> {
        // Print pivot information for each position
        if self.verbose {
            println!("POSITION CREATION FROM SIGNAL");
            println!("Signal Details:");
            println!("  Type: {}", if matches!(signal.position_type, PositionType::Long) { "LONG" } else { "SHORT" });
            println!("  Entry: {:.2}", signal.price);
            println!("  Take Profit: {:.2}", signal.take_profit);
            println!("  Stop Loss: {:.2}", signal.stop_loss);
            println!("  Reason: {}", signal.reason);
            println!("  Strength: {:.2}", signal.strength);
        }
        
        // Get the Fibonacci levels from the signal - already calculated in the signal
        let (limit1_price, limit2_price) = match signal.position_type {
            PositionType::Long => {
                // For longs, limit orders are between entry and stop loss
                // Calculate range between entry and stop loss
                let range = signal.price - signal.stop_loss;
                let limit1 = signal.price - (range * self.config.fib_limit1);
                let limit2 = signal.price - (range * self.config.fib_limit2);
                
                // Add debug for long position limits
                if self.verbose {
                    println!("LONG POSITION LIMIT CALCULATION:");
                    println!("  Range = Entry - SL = {:.2} - {:.2} = {:.2}", 
                            signal.price, signal.stop_loss, range);
                    println!("  Entry Price: {:.2}", signal.price);
                    println!("  Take Profit: {:.2}", signal.take_profit);
                    println!("  Stop Loss: {:.2}", signal.stop_loss);
                    println!("  Limit1 = Entry - Range * {:.2} = {:.2} - {:.2} * {:.2} = {:.2}", 
                            self.config.fib_limit1, signal.price, range, self.config.fib_limit1, limit1);
                    println!("  Limit2 = Entry - Range * {:.2} = {:.2} - {:.2} * {:.2} = {:.2}", 
                            self.config.fib_limit2, signal.price, range, self.config.fib_limit2, limit2);
                    
                    // Validate price order
                    if !(signal.stop_loss < limit2 && limit2 < limit1 && limit1 < signal.price && signal.price < signal.take_profit) {
                        println!("WARNING: Invalid price order for long position limits!");
                    }
                }
                
                (limit1, limit2)
            },
            PositionType::Short => {
                // For shorts, limit orders are between entry and stop loss
                let range = signal.stop_loss - signal.price;
                let limit1 = signal.price + (range * self.config.fib_limit1);
                let limit2 = signal.price + (range * self.config.fib_limit2);
                
                // Add debug for short position limits
                if self.verbose {
                    println!("SHORT POSITION LIMIT CALCULATION:");
                    println!("  Range = SL - Entry = {:.2} - {:.2} = {:.2}", 
                            signal.stop_loss, signal.price, range);
                    println!("  Entry Price: {:.2}", signal.price);
                    println!("  Take Profit: {:.2}", signal.take_profit);
                    println!("  Stop Loss: {:.2}", signal.stop_loss);
                    println!("  Limit1 = Entry + Range * {:.2} = {:.2} + {:.2} * {:.2} = {:.2}", 
                            self.config.fib_limit1, signal.price, range, self.config.fib_limit1, limit1);
                    println!("  Limit2 = Entry + Range * {:.2} = {:.2} + {:.2} * {:.2} = {:.2}", 
                            self.config.fib_limit2, signal.price, range, self.config.fib_limit2, limit2);
                    
                    // Validate price order
                    if !(signal.stop_loss > limit2 && limit2 > limit1 && limit1 > signal.price && signal.price > signal.take_profit) {
                        println!("WARNING: Invalid price order for short position limits!");
                    }
                }
                
                (limit1, limit2)
            }
        };
        
        // Use the position calculator with the spread-adjusted prices
        let result = calculate_positions(
            signal.price,        // Initial entry price
            signal.take_profit,  // Take profit level
            signal.stop_loss,    // Stop loss level
            limit1_price,
            limit2_price,
            account_size,
            risk,
            self.asset_config.leverage,
            signal.position_type.clone(),
            4.0, // h11 default value
            6.0, // h12 default value
        )?;
        
        let position = Position {
            id: uuid::Uuid::new_v4().to_string(),
            symbol: signal.symbol.clone(),
            entry_time: Utc::now().to_string(), // This will be updated when position is actually activated
            entry_price: signal.price,
            size: result.initial_position_size,
            stop_loss: signal.stop_loss,
            take_profit: signal.take_profit,
            position_type: signal.position_type.clone(),
            risk_percent: result.final_risk,
            margin_used: (result.initial_position_size * signal.price) / self.asset_config.leverage,
            status: PositionStatus::Pending, // Mark as pending until price touches entry level
            limit1_price: Some(limit1_price),
            limit2_price: Some(limit2_price),
            limit1_hit: false,
            limit2_hit: false,
            limit1_size: result.limit1_position_size,
            limit2_size: result.limit2_position_size,
            new_tp1: Some(result.new_tp1),
            new_tp2: Some(result.new_tp2),
            entry_order_id: None,
            tp_order_id: None,
            sl_order_id: None,
            limit1_order_id: None,
            limit2_order_id: None,
        };
        
        // Final validation of the created position
        if self.verbose {
            println!("FINAL POSITION VALIDATION:");
            println!("  Type: {}", if matches!(position.position_type, PositionType::Long) { "LONG" } else { "SHORT" });
            println!("  Entry: {:.2}", position.entry_price);
            println!("  Take Profit: {:.2}", position.take_profit);
            println!("  Stop Loss: {:.2}", position.stop_loss);
            println!("  Limit1: {:.2}", position.limit1_price.unwrap_or(0.0));
            println!("  Limit2: {:.2}", position.limit2_price.unwrap_or(0.0));
            println!("  Size: {:.8}", position.size);
            println!("  Limit1 Size: {:.8}", position.limit1_size);
            println!("  Limit2 Size: {:.8}", position.limit2_size);
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