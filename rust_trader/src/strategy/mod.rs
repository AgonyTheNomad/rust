use std::collections::VecDeque;
use serde::{Deserialize, Serialize};
use anyhow::{Context, Result};
use log::*;

pub mod fibonacci;
pub mod pivots;

use fibonacci::FibonacciLevels;
use pivots::PivotPoints;
use crate::models::{Candle, PositionType, Signal};

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
        }
    }
    
    pub fn get_asset_config(&self) -> &AssetConfig {
        &self.asset_config
    }
    
    pub fn initialize_with_history(&mut self, candles: &[Candle]) -> Result<()> {
        debug!("Initializing strategy with {} historical candles", candles.len());
        
        // Process each candle to establish state
        for candle in candles {
            let _ = self.analyze_candle(candle)?;
        }
        
        debug!("Strategy initialized with {} pivot highs and {} pivot lows", 
            self.detected_pivot_highs.len(), self.detected_pivot_lows.len());
        
        Ok(())
    }
    
    pub fn analyze_candle(&mut self, candle: &Candle) -> Result<Vec<Signal>> {
        // Reset signal flags from previous runs
        self.long_signal = false;
        self.short_signal = false;
        
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
        
        // Generate signals
        if self.pivot_high_history.len() >= self.config.signal_lookback + 2 {
            self.generate_signals();
        }
        
        // Also generate from accumulated pivots
        self.generate_signals_from_detected_pivots();
        
        // Create and return signals if generated
        let mut signals = Vec::new();
        
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
                    
                    signals.push(signal);
                    debug!("Generated LONG signal at {}: Entry={}, TP={}, SL={}, Strength={}",
                        candle.time, levels.entry_price, levels.take_profit, levels.stop_loss, strength);
                }
                self.long_signal = false;
            }
        }
        
        if self.short_signal && self.prev_pivot_high.is_some() && self.prev_pivot_low.is_some() {
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
                    
                    signals.push(signal);
                    debug!("Generated SHORT signal at {}: Entry={}, TP={}, SL={}, Strength={}",
                        candle.time, levels.entry_price, levels.take_profit, levels.stop_loss, strength);
                }
                self.short_signal = false;
            }
        }
        
        Ok(signals)
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