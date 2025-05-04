use crate::strategy::Strategy;
use crate::models::{BacktestState, Trade, PositionType, Candle, Position, PositionStatus};
use crate::stats::StatsTracker;
use std::time::{Duration, Instant};
use serde::Serialize;

// ───── Metrics Calculator ───────────────────────────────────────────────

struct MetricsCalculator {
    trades: Vec<Trade>,
    equity_curve: Vec<f64>,
}

impl MetricsCalculator {
    fn new(initial_balance: f64) -> Self {
        Self {
            trades: Vec::new(),
            equity_curve: vec![initial_balance],
        }
    }

    fn add_trade(&mut self, trade: Trade) {
        self.trades.push(trade);
    }

    fn update_equity(&mut self, value: f64) {
        self.equity_curve.push(value);
    }

    fn calculate(&self) -> BacktestMetrics {
        let mut wins = 0;
        let mut losses = 0;
        let mut sum_win = 0.0;
        let mut sum_loss = 0.0;
        let mut total_pnl = 0.0;
        let mut max_win: f64 = 0.0;
        let mut max_loss: f64 = 0.0;

        for t in &self.trades {
            total_pnl += t.pnl;
            if t.pnl > 0.0 {
                wins += 1;
                sum_win += t.pnl;
                max_win = max_win.max(t.pnl);
            } else {
                losses += 1;
                sum_loss += t.pnl.abs();
                max_loss = max_loss.max(t.pnl.abs());
            }
        }

        let total = self.trades.len();
        let win_rate = if total > 0 { wins as f64 / total as f64 } else { 0.0 };
        let profit_factor = if sum_loss > 0.0 { sum_win / sum_loss } else { f64::INFINITY };
        let avg_win = if wins > 0 { sum_win / wins as f64 } else { 0.0 };
        let avg_loss = if losses > 0 { sum_loss / losses as f64 } else { 0.0 };
        let rr = if avg_loss > 0.0 { avg_win / avg_loss } else { f64::INFINITY };

        // drawdown
        let mut peak = self.equity_curve[0];
        let mut max_dd: f64 = 0.0;
        for &eq in &self.equity_curve {
            if eq > peak { peak = eq; }
            else { max_dd = max_dd.max((peak - eq) / peak); }
        }

        BacktestMetrics {
            total_trades: total,
            winning_trades: wins,
            losing_trades: losses,
            win_rate,
            profit_factor,
            total_profit: total_pnl,
            max_drawdown: max_dd,
            sharpe_ratio: 0.0,
            sortino_ratio: 0.0,
            risk_reward_ratio: rr,
            largest_win: max_win,
            largest_loss: max_loss,
            average_win: avg_win,
            average_loss: avg_loss,
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
    metrics: MetricsCalculator,
    verbose: bool,
    ignore_stop_loss: bool, // New flag to control stop loss behavior
}

impl Backtester {
    pub fn new(initial_balance: f64, strategy: Strategy) -> Self {
        Self {
            strategy,
            initial_balance,
            stats: StatsTracker::new(),
            metrics: MetricsCalculator::new(initial_balance),
            verbose: false,
            ignore_stop_loss: false, // Default to using stop losses
        }
    }

    // New method to enable/disable stop losses
    pub fn set_ignore_stop_loss(&mut self, ignore: bool) {
        self.ignore_stop_loss = ignore;
        if self.verbose {
            println!("Stop loss execution is now {}", if ignore { "DISABLED" } else { "ENABLED" });
        }
    }

    pub fn set_verbose(&mut self, v: bool) {
        self.verbose = v;
        self.strategy.set_verbose(v);
    }
    
    // Get a reference to the stats tracker
    pub fn stats(&self) -> &StatsTracker {
        &self.stats
    }

    pub fn run(&mut self, candles: &[Candle]) -> Result<BacktestResults, Box<dyn std::error::Error>> {
        let start = Instant::now();
        // warm-up
        if let Some(warm) = candles.get(0..100) {
            self.strategy.initialize_with_history(warm)?;
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
                    "C{:04}: H={} L={} open?={}",
                    i, candle.high, candle.low, state.position.is_some()
                );
            }

            // update equity curve
            state.equity_curve.push(state.account_balance);
            self.metrics.update_equity(state.account_balance);

            // ─── Exit logic ─────────────────────────────────────────────────────
            if let Some(pos) = &mut state.position {
                if pos.status == PositionStatus::Triggered {
                    pos.status = PositionStatus::Active;
                }
                if pos.status == PositionStatus::Active {
                    if let Some((_t, exit_price, _r)) = self.check_exits(pos, candle) {
                        let pnl = match pos.position_type {
                            PositionType::Long => (exit_price - pos.entry_price) * pos.size,
                            PositionType::Short => (pos.entry_price - exit_price) * pos.size,
                        };
                        state.account_balance += pnl;

                        // record trade
                        let trade = Trade {
                            entry_time:    pos.entry_time.clone(),
                            exit_time:     candle.time.clone(),
                            position_type: if pos.position_type==PositionType::Long { "Long".into() } else { "Short".into() },
                            entry_price:   pos.entry_price,
                            exit_price,
                            size:          pos.size,
                            pnl,
                            risk_percent:  pos.risk_percent,
                            profit_factor: if pnl>0.0 { pnl/(pos.entry_price*pos.size*pos.risk_percent) } else { 0.0 },
                            margin_used:   pos.margin_used,
                            fees:          0.0,
                            slippage:      0.0,
                            stop_loss:     pos.stop_loss,
                            take_profit:   pos.take_profit,
                            limit1_price:  pos.limit1_price,
                            limit2_price:  pos.limit2_price,
                            limit1_hit:    pos.limit1_hit,
                            limit2_hit:    pos.limit2_hit,
                            limit1_time:   pos.limit1_time.clone(),
                            limit2_time:   pos.limit2_time.clone(),
                            exit_tp:       pos.take_profit,
                            new_tp:        pos.new_tp,
                        };
                        state.trades.push(trade.clone());
                        self.stats.record_trade(&trade, state.account_balance);
                        self.metrics.add_trade(trade);
                        state.position = None;
                    }
                }
            }

            // ─── Entry logic ────────────────────────────────────────────────────
            if state.position.is_none() {
                for signal in self.strategy.analyze_candle(candle, false)? {
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

            // ─── Drawdown update ───────────────────────────────────────────────
            state.peak_balance = state.peak_balance.max(state.account_balance);
            state.current_drawdown = (state.peak_balance - state.account_balance) / state.peak_balance;
            state.max_drawdown = state.max_drawdown.max(state.current_drawdown);
        }

        let m = self.metrics.calculate();
        Ok(BacktestResults {
            metrics:       m,
            trades:        state.trades,
            equity_curve:  state.equity_curve,
            duration:      start.elapsed(),
        })
    }

    fn check_exits(&self, position: &mut Position, candle: &Candle)
        -> Option<(String, f64, String)>
    {
        // ─── LIMIT1 LONG ─────────────────────────────────────────────────────
        if !position.limit1_hit
            && position.position_type == PositionType::Long
            && candle.low <= position.limit1_price.unwrap_or(f64::MAX)
        {
            position.limit1_hit = true;
            position.limit1_time = Some(candle.time.clone()); // Record when limit1 was hit
            // **scale in** at limit1_price
            if let Some(fill) = position.limit1_price {
                let old_sz  = position.size;
                let add_sz  = position.limit1_size;
                let avg_old = position.entry_price;
                let avg_new = (avg_old*old_sz + fill*add_sz)/(old_sz+add_sz);
                position.size = old_sz + add_sz;
                position.entry_price = avg_new;
                position.margin_used  = (position.entry_price*position.size)
                    / self.strategy.get_asset_config().leverage;
            }
            // move TP
            if let Some(tp1) = position.new_tp1 {
                position.take_profit = tp1;
                position.new_tp     = Some(tp1);
                if self.verbose { println!("L1 LONG → TP @ ${:.2}", tp1); }
            }
        }

        // ─── LIMIT2 LONG ─────────────────────────────────────────────────────
        if !position.limit2_hit
            && position.position_type == PositionType::Long
            && candle.low <= position.limit2_price.unwrap_or(f64::MAX)
        {
            position.limit2_hit = true;
            position.limit2_time = Some(candle.time.clone()); // Record when limit2 was hit
            if let Some(fill2) = position.limit2_price {
                let old_sz  = position.size;
                let add_sz  = position.limit2_size;
                let avg_old = position.entry_price;
                let avg_new = (avg_old*old_sz + fill2*add_sz)/(old_sz+add_sz);
                position.size = old_sz + add_sz;
                position.entry_price = avg_new;
                position.margin_used  = (position.entry_price*position.size)
                    / self.strategy.get_asset_config().leverage;
            }
            if let Some(tp2) = position.new_tp2 {
                position.take_profit = tp2;
                position.new_tp     = Some(tp2);
                if self.verbose { println!("L2 LONG → TP @ ${:.2}", tp2); }
            }
        }

        // ─── LIMIT1 SHORT ────────────────────────────────────────────────────
        if !position.limit1_hit
            && position.position_type == PositionType::Short
            && candle.high >= position.limit1_price.unwrap_or(f64::MIN)
        {
            position.limit1_hit = true;
            position.limit1_time = Some(candle.time.clone()); // Record when limit1 was hit
            if let Some(fill) = position.limit1_price {
                let old_sz  = position.size;
                let add_sz  = position.limit1_size;
                let avg_old = position.entry_price;
                let avg_new = (avg_old*old_sz + fill*add_sz)/(old_sz+add_sz);
                position.size = old_sz + add_sz;
                position.entry_price = avg_new;
                position.margin_used  = (position.entry_price*position.size)
                    / self.strategy.get_asset_config().leverage;
            }
            if let Some(tp1) = position.new_tp1 {
                position.take_profit = tp1;
                position.new_tp     = Some(tp1);
                if self.verbose { println!("L1 SHORT → TP @ ${:.2}", tp1); }
            }
        }

        // ─── LIMIT2 SHORT ────────────────────────────────────────────────────
        if !position.limit2_hit
            && position.position_type == PositionType::Short
            && candle.high >= position.limit2_price.unwrap_or(f64::MIN)
        {
            position.limit2_hit = true;
            position.limit2_time = Some(candle.time.clone()); // Record when limit2 was hit
            if let Some(fill2) = position.limit2_price {
                let old_sz  = position.size;
                let add_sz  = position.limit2_size;
                let avg_old = position.entry_price;
                let avg_new = (avg_old*old_sz + fill2*add_sz)/(old_sz+add_sz);
                position.size = old_sz + add_sz;
                position.entry_price = avg_new;
                position.margin_used  = (position.entry_price*position.size)
                    / self.strategy.get_asset_config().leverage;
            }
            if let Some(tp2) = position.new_tp2 {
                position.take_profit = tp2;
                position.new_tp     = Some(tp2);
                if self.verbose { println!("L2 SHORT → TP @ ${:.2}", tp2); }
            }
        }

        // ─── FINAL EXIT ──────────────────────────────────────────────────────
        // Take profit hits are always checked
        if position.position_type == PositionType::Long && candle.high >= position.take_profit {
            return Some(("TP".into(), position.take_profit, "hit".into()));
        }
        if position.position_type == PositionType::Short && candle.low <= position.take_profit {
            return Some(("TP".into(), position.take_profit, "hit".into()));
        }
        
        // Stop loss hits are only checked if ignore_stop_loss is false
        if !self.ignore_stop_loss {
            if position.position_type == PositionType::Long && candle.low <= position.stop_loss {
                return Some(("SL".into(), position.stop_loss, "hit".into()));
            }
            if position.position_type == PositionType::Short && candle.high >= position.stop_loss {
                return Some(("SL".into(), position.stop_loss, "hit".into()));
            }
        }
        
        None
    }
}