// src/backtest.rs
use crate::strategy::Strategy;
use crate::models::{BacktestState, Trade, PositionType, Candle};
use crate::metrics::MetricsCalculator;
use crate::stats::StatsTracker;
use std::time::{Duration, Instant};
use serde::Serialize;

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
            self.metrics_calculator.update_equity(state.account_balance, candle.time.parse().unwrap_or_else(|_| chrono::Utc::now()));

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

    fn calculate_metrics(&self, state: &BacktestState) -> BacktestMetrics {
        let perf_metrics = self.metrics_calculator.calculate();
        
        BacktestMetrics {
            total_trades: perf_metrics.total_trades,
            winning_trades: perf_metrics.winning_trades,
            losing_trades: perf_metrics.losing_trades,
            win_rate: perf_metrics.win_rate,
            profit_factor: perf_metrics.profit_factor,
            total_profit: perf_metrics.total_profit,
            max_drawdown: perf_metrics.max_drawdown,
            sharpe_ratio: perf_metrics.sharpe_ratio,
            sortino_ratio: perf_metrics.sortino_ratio,
            risk_reward_ratio: perf_metrics.risk_reward_ratio,
            largest_win: perf_metrics.largest_win,
            largest_loss: perf_metrics.largest_loss,
            average_win: perf_metrics.average_win,
            average_loss: perf_metrics.average_loss,
        }
    }

    pub fn stats(&self) -> &StatsTracker {
        &self.stats
    }
}