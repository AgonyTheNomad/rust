use std::collections::{VecDeque, HashMap};
use serde::{Deserialize, Serialize};
use anyhow::Result;
use log::*;
use chrono::{DateTime, Utc, Duration, Timelike}; // Added Timelike trait import here

pub mod fibonacci;
pub mod pivots;

use fibonacci::FibonacciLevels;
use pivots::PivotPoints;
use crate::models::{Candle, PositionType, Signal, Position, PositionStatus};

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
    
    // New fields for handling only most recent candle signals
    last_processed_minute: Option<DateTime<Utc>>,
    last_signal_time: HashMap<String, DateTime<Utc>>,
    signal_cooldown: Duration,
    max_signal_age_seconds: i64,
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
            
            // Initialize new fields
            last_processed_minute: None,
            last_signal_time: HashMap::new(),
            signal_cooldown: Duration::minutes(5),  // Don't generate signals for the same symbol more often than every 5 minutes
            max_signal_age_seconds: 120,  // Signals older than 2 minutes are considered stale
        }
    }
    
    pub fn get_asset_config(&self) -> &AssetConfig {
        &self.asset_config
    }
    
    pub fn initialize_with_history(&mut self, candles: &[Candle]) -> Result<()> {
        debug!("Initializing strategy with {} historical candles", candles.len());
        
        // Process each candle to establish state, but don't generate signals during initialization
        for candle in candles {
            // Call analyze_candle_internal which just updates state but doesn't generate signals
            self.analyze_candle_internal(candle)?;
        }
        
        debug!("Strategy initialized with {} pivot highs and {} pivot lows", 
            self.detected_pivot_highs.len(), self.detected_pivot_lows.len());
        
        Ok(())
    }
    
    // New internal method that just updates state without generating signals
    fn analyze_candle_internal(&mut self, candle: &Candle) -> Result<()> {
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
            self.detected_pivot_highs.push(high);
            self.prev_pivot_high = Some(high);
        }
        
        if let Some(low) = pivot_low {
            self.detected_pivot_lows.push(low);
            self.prev_pivot_low = Some(low);
        }
        
        Ok(())
    }
    
    // Helper function to calculate take profit adjustments
    fn calculate_tp_adjustment(&self, entry: f64, tp: f64, limit: f64, is_long: bool) -> f64 {
        // For long positions
        if is_long {
            // If take profit is above entry (which it should be for longs)
            if tp > entry {
                // Calculate the distance between entry and original TP
                let tp_distance = tp - entry;
                
                // Calculate the new TP based on limit level
                // For limit1, move TP closer to limit1 (4:1 ratio)
                // For limit2, move TP even closer to limit2 (6:1 ratio)
                let ratio = if (limit - entry).abs() > (limit - entry).abs() * 0.75 {
                    // This is likely limit2
                    6.0
                } else {
                    // This is likely limit1
                    4.0
                };
                
                // Calculate adjusted TP
                limit + (tp_distance / ratio)
            } else {
                // Fallback if TP is not above entry
                tp
            }
        } 
        // For short positions
        else {
            // If take profit is below entry (which it should be for shorts)
            if tp < entry {
                // Calculate the distance between entry and original TP
                let tp_distance = entry - tp;
                
                // Calculate the new TP based on limit level
                // For limit1, move TP closer to limit1 (4:1 ratio)
                // For limit2, move TP even closer to limit2 (6:1 ratio)
                let ratio = if (limit - entry).abs() > (limit - entry).abs() * 0.75 {
                    // This is likely limit2
                    6.0
                } else {
                    // This is likely limit1
                    4.0
                };
                
                // Calculate adjusted TP
                limit - (tp_distance / ratio)
            } else {
                // Fallback if TP is not below entry
                tp
            }
        }
    }
    
    pub fn analyze_candle(&mut self, candle: &Candle) -> Result<Vec<(Signal, Position)>> {
        // First update internal state
        self.analyze_candle_internal(candle)?;
        
        // Get the candle's timestamp
        let candle_time = DateTime::parse_from_rfc3339(&candle.time)
            .map_err(|e| anyhow::anyhow!("Failed to parse candle time: {}", e))?
            .with_timezone(&Utc);
        
        // Get current time
        let now = Utc::now();
        
        // Calculate the difference (in seconds)
        let age_seconds = (now - candle_time).num_seconds();
        
        // Only generate signals if this is a recent candle
        if age_seconds > self.max_signal_age_seconds {
            debug!("Skipping signal generation - candle too old: {} seconds", age_seconds);
            return Ok(Vec::new());
        }
        
        // Extract minute from timestamp (truncate to minute)
        let candle_minute = candle_time
            .with_nanosecond(0).unwrap()
            .with_second(0).unwrap();
        
        // Check if this is a new minute
        let is_new_minute = match self.last_processed_minute {
            Some(last_minute) => candle_minute > last_minute,
            None => true,
        };
        
        // Update the last processed minute
        self.last_processed_minute = Some(candle_minute);
        
        // Only generate signals on new minute candles
        if !is_new_minute {
            debug!("Not a new minute, skipping signal generation");
            return Ok(Vec::new());
        }
        
        // Check signal cooldown
        let symbol = &self.asset_config.name;
        if let Some(last_time) = self.last_signal_time.get(symbol) {
            if now - *last_time < self.signal_cooldown {
                debug!("Skipping signal generation - in cooldown period for {}", symbol);
                return Ok(Vec::new());
            }
        }
        
        // Reset signal flags
        self.long_signal = false;
        self.short_signal = false;
        
        // Parse high and low
        let high = candle.high;
        let low = candle.low;
        
        // Generate signals
        if self.pivot_high_history.len() >= self.config.signal_lookback + 2 {
            self.generate_signals();
        }
        
        // Also generate from accumulated pivots
        self.generate_signals_from_detected_pivots();
        
        // Create and return signals if generated
        let mut signal_positions = Vec::new();
        
        // Process long signals
        if self.long_signal && self.prev_pivot_high.is_some() && self.prev_pivot_low.is_some() {
            if let Some(levels) = self.fib.calculate_long_levels(
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
                    
                    // Create a position object with the price levels and scaling ratios
                    let position = Position {
                        id: uuid::Uuid::new_v4().to_string(),
                        symbol: self.asset_config.name.clone(),
                        entry_time: Utc::now(),
                        entry_price: levels.entry_price,
                        size: 0.0, // No default size - this will be calculated by risk management
                        stop_loss: levels.stop_loss,
                        take_profit: levels.take_profit,
                        position_type: PositionType::Long,
                        risk_percent: 0.0, // No default risk - will be calculated by risk management
                        margin_used: 0.0,
                        status: PositionStatus::Pending,
                        limit1_price: Some(levels.limit1),
                        limit2_price: Some(levels.limit2),
                        limit1_hit: false,
                        limit2_hit: false,
                        // Store the scaling ratios, not absolute sizes
                        limit1_size: 3.0, // This represents a 3:1 ratio for scaling
                        limit2_size: 5.0, // This represents a 5:1 ratio for scaling
                        new_tp1: Some(self.calculate_tp_adjustment(levels.entry_price, levels.take_profit, levels.limit1, true)),
                        new_tp2: Some(self.calculate_tp_adjustment(levels.entry_price, levels.take_profit, levels.limit2, true)),
                        entry_order_id: None,
                        tp_order_id: None,
                        sl_order_id: None,
                        limit1_order_id: None,
                        limit2_order_id: None,
                    };
                    
                    signal_positions.push((signal, position));
                    debug!("Generated LONG signal at {}: Entry={}, TP={}, SL={}, Strength={}",
                        candle.time, levels.entry_price, levels.take_profit, levels.stop_loss, strength);
                    
                    // Update last signal time
                    self.last_signal_time.insert(self.asset_config.name.clone(), now);
                }
                self.long_signal = false;
            }
        }
        
        // Process short signals (only if we didn't generate a long signal)
        if signal_positions.is_empty() && self.short_signal && self.prev_pivot_high.is_some() && self.prev_pivot_low.is_some() {
            if let Some(levels) = self.fib.calculate_short_levels(
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
                    
                    // Create a position object with the price levels and scaling ratios
                    let position = Position {
                        id: uuid::Uuid::new_v4().to_string(),
                        symbol: self.asset_config.name.clone(),
                        entry_time: Utc::now(),
                        entry_price: levels.entry_price,
                        size: 0.0, // No default size - this will be calculated by risk management
                        stop_loss: levels.stop_loss,
                        take_profit: levels.take_profit,
                        position_type: PositionType::Short,
                        risk_percent: 0.0, // No default risk - will be calculated by risk management
                        margin_used: 0.0,
                        status: PositionStatus::Pending,
                        limit1_price: Some(levels.limit1),
                        limit2_price: Some(levels.limit2),
                        limit1_hit: false,
                        limit2_hit: false,
                        // Store the scaling ratios, not absolute sizes
                        limit1_size: 3.0, // This represents a 3:1 ratio for scaling
                        limit2_size: 5.0, // This represents a 5:1 ratio for scaling
                        new_tp1: Some(self.calculate_tp_adjustment(levels.entry_price, levels.take_profit, levels.limit1, false)),
                        new_tp2: Some(self.calculate_tp_adjustment(levels.entry_price, levels.take_profit, levels.limit2, false)),
                        entry_order_id: None,
                        tp_order_id: None,
                        sl_order_id: None,
                        limit1_order_id: None,
                        limit2_order_id: None,
                    };
                    
                    signal_positions.push((signal, position));
                    debug!("Generated SHORT signal at {}: Entry={}, TP={}, SL={}, Strength={}",
                        candle.time, levels.entry_price, levels.take_profit, levels.stop_loss, strength);
                    
                    // Update last signal time
                    self.last_signal_time.insert(self.asset_config.name.clone(), now);
                }
                self.short_signal = false;
            }
        }
        
        Ok(signal_positions)
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
                self.long_signal = true;
            }
        }
        
        if let (Some(prev), Some(curr)) = (prev_pivot_low, curr_pivot_low) {
            if curr < prev {
                self.short_signal = true;
            }
        }
    }
    
    fn generate_signals_from_detected_pivots(&mut self) {
        if self.detected_pivot_highs.len() >= 2 {
            let latest = self.detected_pivot_highs[self.detected_pivot_highs.len() - 1];
            let previous = self.detected_pivot_highs[self.detected_pivot_highs.len() - 2];
            if latest > previous {
                self.long_signal = true;
            }
        }
        
        if self.detected_pivot_lows.len() >= 2 {
            let latest = self.detected_pivot_lows[self.detected_pivot_lows.len() - 1];
            let previous = self.detected_pivot_lows[self.detected_pivot_lows.len() - 2];
            if latest < previous {
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
        
        // Ensure strength is between 0 and 1
        strength.max(0.0).min(1.0)
    }
}