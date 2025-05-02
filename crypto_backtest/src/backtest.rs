// src/backtest.rs
use crate::strategy::Strategy;
use crate::models::{BacktestState, Trade, PositionType, Candle};
use crate::stats::StatsTracker;
use std::time::{Duration, Instant};
use serde::Serialize;
use chrono::Utc;

// Define a local implementation of metrics for backtest.rs
// This addresses the import issue while maintaining functionality
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
        let mut largest_win: f64 = 0.0;  // Explicitly typed as f64
        let mut largest_loss: f64 = 0.0;  // Explicitly typed as f64
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
        let mut max_drawdown: f64 = 0.0;  // Explicitly typed as f64
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
}

impl Backtester {
    pub fn new(initial_balance: f64, strategy: Strategy) -> Self {
        let metrics_calculator = MetricsCalculator::new(initial_balance, 0.02);
        
        Self {
            strategy,
            initial_balance,
            stats: StatsTracker::new(),
            metrics_calculator,
        }
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
        for candle in candles {
            // Track equity curve
            state.equity_curve.push(state.account_balance);
            self.metrics_calculator.update_equity(
                state.account_balance, 
                candle.time.parse().unwrap_or_else(|_| Utc::now())
            );

            // Analyze candle with strategy and get signals
            let signals = self.strategy.analyze_candle(candle)?;
            
            // Process signals to create trades
            for signal in signals {
                if let Ok(position) = self.strategy.create_scaled_position(&signal, state.account_balance, 0.02) {
                    // Create trade from position
                    let trade = Trade {
                        entry_time: position.entry_time.clone(),
                        exit_time: candle.time.clone(),
                        position_type: match position.position_type {
                            PositionType::Long => "Long".to_string(),
                            PositionType::Short => "Short".to_string(),
                        },
                        entry_price: position.entry_price,
                        exit_price: candle.close,
                        size: position.size,
                        pnl: (candle.close - position.entry_price) * position.size,
                        risk_percent: position.risk_percent,
                        profit_factor: 1.0,
                        margin_used: position.margin_used,
                        fees: 0.0,
                        slippage: 0.0,
                    };
                    
                    state.trades.push(trade.clone());
                    self.stats.record_trade(&trade, state.account_balance);
                    self.metrics_calculator.add_trade(trade);
                }
            }

            // Update drawdown
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

    fn calculate_metrics(&self, _state: &BacktestState) -> BacktestMetrics {
        // Added underscore to indicate the state parameter is intentionally unused
        self.metrics_calculator.calculate()
    }

    pub fn stats(&self) -> &StatsTracker {
        &self.stats
    }
}