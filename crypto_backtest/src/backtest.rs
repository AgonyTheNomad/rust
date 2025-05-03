// src/backtest.rs
use crate::strategy::Strategy;
use crate::models::{BacktestState, Trade, PositionType, Candle, Signal, Position, PositionStatus};
use crate::stats::StatsTracker;
use std::time::{Duration, Instant};
use serde::Serialize;
use chrono::Utc;

// Define a local implementation of metrics for backtest.rs
struct MetricsCalculator {
    initial_balance: f64,
    risk_free_rate: f64,
    trades: Vec<Trade>,
    equity_curve: Vec<f64>,
    timestamps: Vec<chrono::DateTime<chrono::Utc>>,
}

impl MetricsCalculator {
    fn new(initial_balance: f64, risk_free_rate: f64) -> Self {
        Self {
            initial_balance,
            risk_free_rate,
            trades: Vec::new(),
            equity_curve: vec![initial_balance],
            timestamps: Vec::new(),
        }
    }

    fn add_trade(&mut self, trade: Trade) {
        self.trades.push(trade);
    }

    fn update_equity(&mut self, value: f64, timestamp: chrono::DateTime<chrono::Utc>) {
        self.equity_curve.push(value);
        self.timestamps.push(timestamp);
    }
    
    fn calculate(&self) -> BacktestMetrics {
        let mut winning_trades = 0;
        let mut losing_trades = 0;
        let mut total_profit = 0.0;
        let mut largest_win: f64 = 0.0;
        let mut largest_loss: f64 = 0.0;
        let mut total_wins = 0.0;
        let mut total_losses = 0.0;

        for trade in &self.trades {
            if trade.pnl > 0.0 {
                winning_trades += 1;
                total_wins += trade.pnl;
                largest_win = largest_win.max(trade.pnl);
            } else {
                losing_trades += 1;
                total_losses += trade.pnl.abs();
                largest_loss = largest_loss.max(trade.pnl.abs());
            }

            total_profit += trade.pnl;
        }

        let total_trades = self.trades.len();
        let win_rate = if total_trades > 0 {
            winning_trades as f64 / total_trades as f64
        } else {
            0.0
        };

        let profit_factor = if total_losses > 0.0 {
            total_wins / total_losses
        } else {
            f64::INFINITY
        };

        let average_win = if winning_trades > 0 {
            total_wins / winning_trades as f64
        } else {
            0.0
        };

        let average_loss = if losing_trades > 0 {
            total_losses / losing_trades as f64
        } else {
            0.0
        };

        // Simple risk/reward ratio
        let risk_reward_ratio = if average_loss > 0.0 {
            average_win / average_loss
        } else {
            f64::INFINITY
        };

        // Simple calculation for max drawdown
        let mut max_drawdown: f64 = 0.0;
        let mut peak = self.equity_curve[0];
        
        for &equity in &self.equity_curve {
            if equity > peak {
                peak = equity;
            } else {
                let drawdown = (peak - equity) / peak;
                max_drawdown = max_drawdown.max(drawdown);
            }
        }

        BacktestMetrics {
            total_trades,
            winning_trades,
            losing_trades,
            win_rate,
            profit_factor,
            total_profit,
            max_drawdown,
            sharpe_ratio: 0.0, // Simplified
            sortino_ratio: 0.0, // Simplified
            risk_reward_ratio,
            largest_win,
            largest_loss,
            average_win,
            average_loss,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BacktestMetrics {
    pub total_trades: usize,
    pub winning_trades: usize,
    pub losing_trades: usize,
    pub win_rate: f64,
    pub profit_factor: f64,
    pub total_profit: f64,
    pub max_drawdown: f64,
    pub sharpe_ratio: f64,
    pub sortino_ratio: f64,
    pub risk_reward_ratio: f64,
    pub largest_win: f64,
    pub largest_loss: f64,
    pub average_win: f64,
    pub average_loss: f64,
}

#[derive(Debug)]
pub struct BacktestResults {
    pub metrics: BacktestMetrics,
    pub trades: Vec<Trade>,
    pub equity_curve: Vec<f64>,
    pub duration: Duration,
}

pub struct Backtester {
    strategy: Strategy,
    initial_balance: f64,
    stats: StatsTracker,
    metrics_calculator: MetricsCalculator,
    verbose: bool, // Flag to control debug output
}

impl Backtester {
    pub fn new(initial_balance: f64, strategy: Strategy) -> Self {
        let metrics_calculator = MetricsCalculator::new(initial_balance, 0.02);
        
        Self {
            strategy,
            initial_balance,
            stats: StatsTracker::new(),
            metrics_calculator,
            verbose: false, // Default to false, can be enabled with set_verbose
        }
    }
    
    // Method to enable or disable verbose logging
    pub fn set_verbose(&mut self, verbose: bool) {
        self.verbose = verbose;
    }

    pub fn run(&mut self, candles: &[Candle]) -> Result<BacktestResults, Box<dyn std::error::Error>> {
        let start_time = Instant::now();
        
        let mut state = BacktestState {
            account_balance: self.initial_balance,
            initial_balance: self.initial_balance,
            position: None,
            equity_curve: vec![self.initial_balance],
            trades: Vec::new(),
            max_drawdown: 0.0,
            peak_balance: self.initial_balance,
            current_drawdown: 0.0,
        };

        // Process all candles
        for (i, candle) in candles.iter().enumerate() {
            // Enhanced debugging - print state at beginning of each candle
            if self.verbose && i % 100 == 0 {
                println!("Candle {}: O=${:.2} H=${:.2} L=${:.2} C=${:.2} - Has Position: {}",
                    candle.time, candle.open, candle.high, candle.low, candle.close, 
                    state.position.is_some());
            }
            
            // Track equity curve
            state.equity_curve.push(state.account_balance);
            let candle_time = candle.time.parse().unwrap_or_else(|_| Utc::now());
            self.metrics_calculator.update_equity(state.account_balance, candle_time);
            
            // Check if we need to close any existing active positions
            if let Some(position) = &mut state.position {
                let exit_info = self.check_exits(position, candle);
                
                if let Some((exit_type, exit_price, exit_reason)) = exit_info {
                    // Calculate PnL
                    let pnl = match position.position_type {
                        PositionType::Long => (exit_price - position.entry_price) * position.size,
                        PositionType::Short => (position.entry_price - exit_price) * position.size,
                    };
                    
                    // Update account balance
                    state.account_balance += pnl;
                    
                    // Create trade record
                    let trade = Trade {
                        entry_time: position.entry_time.clone(),
                        exit_time: candle.time.clone(),
                        position_type: if matches!(position.position_type, PositionType::Long) {
                            "Long".to_string()
                        } else {
                            "Short".to_string()
                        },
                        entry_price: position.entry_price,
                        exit_price,
                        size: position.size,
                        pnl,
                        risk_percent: position.risk_percent,
                        profit_factor: if pnl > 0.0 {
                            pnl / (position.entry_price * position.size * position.risk_percent)
                        } else {
                            0.0
                        },
                        margin_used: position.margin_used,
                        fees: 0.0,
                        slippage: 0.0,
                    };
                    
                    // Log trade closure
                    if self.verbose {
                        println!("CLOSED {} POSITION: {} @ ${:.2}, Exit: {} @ ${:.2}, PnL: ${:.2}, Reason: {}",
                            trade.position_type, position.entry_time, position.entry_price,
                            candle.time, exit_price, pnl, exit_reason);
                    }
                    
                    // Add to trade list and record in stats
                    state.trades.push(trade.clone());
                    self.stats.record_trade(&trade, state.account_balance);
                    self.metrics_calculator.add_trade(trade);
                    
                    // Clear position
                    state.position = None;
                }
            }

            // Generate new signals only if no active position
            let has_active_position = state.position.is_some();
            
            // Analyze candle for new signals
            match self.strategy.analyze_candle(candle, has_active_position) {
                Ok(signals) => {
                    // Only process signals if we don't have an active position
                    if !has_active_position && !signals.is_empty() {
                        for signal in signals {
                            // Create a position directly from the signal
                            match self.strategy.create_scaled_position(&signal, state.account_balance, 0.02) {
                                Ok(mut position) => {
                                    // Set the entry time to the current candle
                                    position.entry_time = candle.time.clone();
                                    position.status = PositionStatus::Active;
                                    
                                    // Log position creation
                                    if self.verbose {
                                        println!("OPENED {} POSITION FROM SIGNAL: Entry @ ${:.2}, SL: ${:.2}, TP: ${:.2}, Time: {}",
                                            if matches!(position.position_type, PositionType::Long) {
                                                "LONG"
                                            } else {
                                                "SHORT"
                                            },
                                            position.entry_price,
                                            position.stop_loss,
                                            position.take_profit,
                                            candle.time);
                                    }
                                    
                                    // Set as active position
                                    state.position = Some(position);
                                    
                                    // Only open one position at a time
                                    break;
                                },
                                Err(e) => {
                                    if self.verbose {
                                        println!("Failed to create position from signal: {}", e);
                                    }
                                }
                            }
                        }
                    }
                },
                Err(e) => {
                    if self.verbose {
                        println!("Error analyzing candle: {}", e);
                    }
                    continue;
                }
            };

            // Update drawdown calculations
            state.peak_balance = state.peak_balance.max(state.account_balance);
            state.current_drawdown = (state.peak_balance - state.account_balance) / state.peak_balance;
            state.max_drawdown = state.max_drawdown.max(state.current_drawdown);
        }

        // Calculate final metrics
        let metrics = self.calculate_metrics(&state);
        
        Ok(BacktestResults {
            metrics,
            trades: state.trades,
            equity_curve: state.equity_curve,
            duration: start_time.elapsed(),
        })
    }

    // Check if position should be closed (hit stop loss, take profit, or limit orders)
    fn check_exits(&self, position: &mut Position, candle: &Candle) -> Option<(String, f64, String)> {
        // Check for stop loss for long positions
        if matches!(position.position_type, PositionType::Long) && candle.low <= position.stop_loss {
            return Some(("Stop Loss".to_string(), position.stop_loss, "Price hit stop loss level".to_string()));
        }
        
        // Check for stop loss for short positions
        if matches!(position.position_type, PositionType::Short) && candle.high >= position.stop_loss {
            return Some(("Stop Loss".to_string(), position.stop_loss, "Price hit stop loss level".to_string()));
        }
        
        // Check for take profit for long positions
        if matches!(position.position_type, PositionType::Long) && candle.high >= position.take_profit {
            return Some(("Take Profit".to_string(), position.take_profit, "Price hit take profit level".to_string()));
        }
        
        // Check for take profit for short positions
        if matches!(position.position_type, PositionType::Short) && candle.low <= position.take_profit {
            return Some(("Take Profit".to_string(), position.take_profit, "Price hit take profit level".to_string()));
        }
        
        // Handle limit orders - Check if limit1 is hit for long positions
        if !position.limit1_hit && 
           matches!(position.position_type, PositionType::Long) && 
           candle.low <= position.limit1_price.unwrap_or(f64::MAX) {
            // Mark limit1 as hit
            position.limit1_hit = true;
            
            // Update take profit if new_tp1 is set
            if let Some(new_tp) = position.new_tp1 {
                position.take_profit = new_tp;
                
                if self.verbose {
                    println!("LIMIT1 HIT for LONG position: Updated TP to ${:.2}", new_tp);
                }
            }
        }
        
        // Check if limit2 is hit for long positions
        if !position.limit2_hit && 
           matches!(position.position_type, PositionType::Long) && 
           candle.low <= position.limit2_price.unwrap_or(f64::MAX) {
            // Mark limit2 as hit
            position.limit2_hit = true;
            
            // Update take profit if new_tp2 is set
            if let Some(new_tp) = position.new_tp2 {
                position.take_profit = new_tp;
                
                if self.verbose {
                    println!("LIMIT2 HIT for LONG position: Updated TP to ${:.2}", new_tp);
                }
            }
        }
        
        // Check if limit1 is hit for short positions
        if !position.limit1_hit && 
           matches!(position.position_type, PositionType::Short) && 
           candle.high >= position.limit1_price.unwrap_or(0.0) {
            // Mark limit1 as hit
            position.limit1_hit = true;
            
            // Update take profit if new_tp1 is set
            if let Some(new_tp) = position.new_tp1 {
                position.take_profit = new_tp;
                
                if self.verbose {
                    println!("LIMIT1 HIT for SHORT position: Updated TP to ${:.2}", new_tp);
                }
            }
        }
        
        // Check if limit2 is hit for short positions
        if !position.limit2_hit && 
           matches!(position.position_type, PositionType::Short) && 
           candle.high >= position.limit2_price.unwrap_or(0.0) {
            // Mark limit2 as hit
            position.limit2_hit = true;
            
            // Update take profit if new_tp2 is set
            if let Some(new_tp) = position.new_tp2 {
                position.take_profit = new_tp;
                
                if self.verbose {
                    println!("LIMIT2 HIT for SHORT position: Updated TP to ${:.2}", new_tp);
                }
            }
        }
        
        // No exit conditions met
        None
    }

    fn calculate_metrics(&self, _state: &BacktestState) -> BacktestMetrics {
        // Added underscore to indicate the state parameter is intentionally unused
        self.metrics_calculator.calculate()
    }

    pub fn stats(&self) -> &StatsTracker {
        &self.stats
    }
}