use crate::strategy::Strategy;
use crate::models::{BacktestState, Trade, PositionType, Candle, Position, PositionStatus};
use crate::stats::StatsTracker;
use std::time::{Duration, Instant};
use serde::Serialize;
use chrono::Utc;

// ───── Metrics Calculator ───────────────────────────────────────────────

struct MetricsCalculator {
    initial_balance: f64,
    risk_free_rate: f64,
    trades: Vec<Trade>,
    equity_curve: Vec<f64>,
    timestamps: Vec<chrono::DateTime<Utc>>,
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

    fn update_equity(&mut self, value: f64, timestamp: chrono::DateTime<Utc>) {
        self.equity_curve.push(value);
        self.timestamps.push(timestamp);
    }

    fn calculate(&self) -> BacktestMetrics {
        let mut winning_trades: usize = 0;
        let mut losing_trades: usize = 0;
        let mut total_wins: f64 = 0.0;
        let mut total_losses: f64 = 0.0;
        let mut total_profit: f64 = 0.0;
        let mut largest_win: f64 = 0.0;
        let mut largest_loss: f64 = 0.0;

        for trade in &self.trades {
            total_profit += trade.pnl;
            if trade.pnl > 0.0 {
                winning_trades += 1;
                total_wins += trade.pnl;
                largest_win = largest_win.max(trade.pnl);
            } else {
                losing_trades += 1;
                total_losses += trade.pnl.abs();
                largest_loss = largest_loss.max(trade.pnl.abs());
            }
        }

        let total_trades = self.trades.len();
        let win_rate: f64 = if total_trades > 0 {
            winning_trades as f64 / total_trades as f64
        } else {
            0.0
        };

        let profit_factor: f64 = if total_losses > 0.0 {
            total_wins / total_losses
        } else {
            f64::INFINITY
        };

        let average_win: f64 = if winning_trades > 0 {
            total_wins / winning_trades as f64
        } else {
            0.0
        };

        let average_loss: f64 = if losing_trades > 0 {
            total_losses / losing_trades as f64
        } else {
            0.0
        };

        let risk_reward_ratio: f64 = if average_loss > 0.0 {
            average_win / average_loss
        } else {
            f64::INFINITY
        };

        // Max drawdown
        let mut peak: f64 = self.equity_curve[0];
        let mut max_dd: f64 = 0.0;
        for &eq in &self.equity_curve {
            if eq > peak {
                peak = eq;
            } else {
                max_dd = max_dd.max((peak - eq) / peak);
            }
        }

        BacktestMetrics {
            total_trades,
            winning_trades,
            losing_trades,
            win_rate,
            profit_factor,
            total_profit,
            max_drawdown: max_dd,
            sharpe_ratio: 0.0,
            sortino_ratio: 0.0,
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
    verbose: bool,
}

impl Backtester {
    pub fn new(initial_balance: f64, strategy: Strategy) -> Self {
        Self {
            strategy,
            initial_balance,
            stats: StatsTracker::new(),
            metrics_calculator: MetricsCalculator::new(initial_balance, 0.02),
            verbose: false,
        }
    }

    pub fn set_verbose(&mut self, v: bool) {
        self.verbose = v;
        self.strategy.set_verbose(v);
    }

    pub fn print_pending_orders(&self) {
        let orders = self.strategy.get_pending_orders_info();
        println!("Pending orders: {}", orders.len());
        for (i, o) in orders.iter().enumerate() {
            println!("  {}. {}", i + 1, o);
        }
    }

    pub fn run(
        &mut self,
        candles: &[Candle],
    ) -> Result<BacktestResults, Box<dyn std::error::Error>> {
        let start_time = Instant::now();
        // Warmup history if needed
        if let Some(warmup) = candles.get(0..100) {
            self.strategy.initialize_with_history(warmup)?;
        }

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

        for (i, candle) in candles.iter().enumerate() {
            if self.verbose && i % 100 == 0 {
                println!(
                    "Candle {}: H={}, L={} | HasPos={}",
                    candle.time,
                    candle.high,
                    candle.low,
                    state.position.is_some()
                );
            }

            state.equity_curve.push(state.account_balance);
            let ts = candle
                .time
                .parse()
                .unwrap_or_else(|_| Utc::now());
            self.metrics_calculator
                .update_equity(state.account_balance, ts);

            // ─── Exit existing position ────────────────────────────────────────
            if let Some(pos) = &mut state.position {
                if pos.status == PositionStatus::Triggered {
                    pos.status = PositionStatus::Active;
                }
                if pos.status == PositionStatus::Active {
                    if let Some((_exit_type, exit_price, _exit_reason)) =
                        self.check_exits(pos, candle)
                    {
                        let pnl = match pos.position_type {
                            PositionType::Long => {
                                (exit_price - pos.entry_price) * pos.size
                            }
                            PositionType::Short => {
                                (pos.entry_price - exit_price) * pos.size
                            }
                        };
                        state.account_balance += pnl;

                        let trade = Trade {
                            entry_time:      pos.entry_time.clone(),
                            exit_time:       candle.time.clone(),
                            position_type:   if pos.position_type==PositionType::Long { "Long".into() } else { "Short".into() },
                            entry_price:     pos.entry_price,
                            exit_price,
                            exit_tp:         pos.take_profit,      // ← newly required
                            size:            pos.size,
                            pnl,
                            risk_percent:    pos.risk_percent,
                            profit_factor:   if pnl > 0.0 { pnl / (pos.entry_price * pos.size * pos.risk_percent) } else { 0.0 },
                            margin_used:     pos.margin_used,
                            fees:            0.0,
                            slippage:        0.0,
                            limit1_hit:      pos.limit1_hit,
                            limit2_hit:      pos.limit2_hit,
                            limit1_time:     pos.limit1_time.clone(),
                            limit2_time:     pos.limit2_time.clone(),
                            new_tp:          pos.new_tp,
                        };
                        
                        

                        state.trades.push(trade.clone());
                        self.stats
                            .record_trade(&trade, state.account_balance);
                        self.metrics_calculator.add_trade(trade);
                        state.position = None;
                    }
                }
            }

            // ─── Enter new position ────────────────────────────────────────────
            if state.position.is_none() {
                for signal in
                    self.strategy.analyze_candle(candle, false)?
                {
                    let mut pos = self.strategy.create_scaled_position(
                        &signal,
                        state.account_balance,
                        self.strategy.get_max_risk_per_trade(),
                    )?;
                    pos.entry_time = candle.time.clone();
                    state.position = Some(pos);
                    break;
                }
            }

            // ─── Update drawdowns ──────────────────────────────────────────────
            state.peak_balance = state.peak_balance.max(state.account_balance);
            state.current_drawdown =
                (state.peak_balance - state.account_balance) / state.peak_balance;
            state.max_drawdown =
                state.max_drawdown.max(state.current_drawdown);
        }

        let metrics = self.calculate_metrics(&state);
        Ok(BacktestResults {
            metrics,
            trades: state.trades,
            equity_curve: state.equity_curve,
            duration: start_time.elapsed(),
        })
    }

    fn check_exits(
        &self,
        position: &mut Position,
        candle: &Candle,
    ) -> Option<(String, f64, String)> {
        // LIMIT1 LONG
        if !position.limit1_hit
            && position.position_type == PositionType::Long
            && candle.low <= position.limit1_price.unwrap_or(f64::MAX)
        {
            position.limit1_hit = true;
            if let Some(tp1) = position.new_tp1 {
                position.take_profit = tp1;
                position.new_tp = Some(tp1);
                if self.verbose {
                    println!("L1 LONG: TP→${:.2}", tp1);
                }
            }
        }
        // LIMIT2 LONG
        if !position.limit2_hit
            && position.position_type == PositionType::Long
            && candle.low <= position.limit2_price.unwrap_or(f64::MAX)
        {
            position.limit2_hit = true;
            if let Some(tp2) = position.new_tp2 {
                position.take_profit = tp2;
                position.new_tp = Some(tp2);
                if self.verbose {
                    println!("L2 LONG: TP→${:.2}", tp2);
                }
            }
        }
        // LIMIT1 SHORT
        if !position.limit1_hit
            && position.position_type == PositionType::Short
            && candle.high >= position.limit1_price.unwrap_or(f64::MIN)
        {
            position.limit1_hit = true;
            if let Some(tp1) = position.new_tp1 {
                position.take_profit = tp1;
                position.new_tp = Some(tp1);
                if self.verbose {
                    println!("L1 SHORT: TP→${:.2}", tp1);
                }
            }
        }
        // LIMIT2 SHORT
        if !position.limit2_hit
            && position.position_type == PositionType::Short
            && candle.high >= position.limit2_price.unwrap_or(f64::MIN)
        {
            position.limit2_hit = true;
            if let Some(tp2) = position.new_tp2 {
                position.take_profit = tp2;
                position.new_tp = Some(tp2);
                if self.verbose {
                    println!("L2 SHORT: TP→${:.2}", tp2);
                }
            }
        }

        // ─── Final exit logic ───────────────────────────────────────────────
        if position.position_type == PositionType::Long
            && candle.high >= position.take_profit
        {
            return Some((
                "Take Profit".to_string(),
                position.take_profit,
                "Hit TP".to_string(),
            ));
        }
        if position.position_type == PositionType::Long
            && candle.low <= position.stop_loss
        {
            return Some((
                "Stop Loss".to_string(),
                position.stop_loss,
                "Hit SL".to_string(),
            ));
        }
        if position.position_type == PositionType::Short
            && candle.low <= position.take_profit
        {
            return Some((
                "Take Profit".to_string(),
                position.take_profit,
                "Hit TP".to_string(),
            ));
        }
        if position.position_type == PositionType::Short
            && candle.high >= position.stop_loss
        {
            return Some((
                "Stop Loss".to_string(),
                position.stop_loss,
                "Hit SL".to_string(),
            ));
        }
        None
    }

    fn calculate_metrics(&self, _state: &BacktestState) -> BacktestMetrics {
        self.metrics_calculator.calculate()
    }

    pub fn stats(&self) -> &StatsTracker {
        &self.stats
    }
}
