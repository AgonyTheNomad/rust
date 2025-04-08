use crate::models::{Candle, Trade, BacktestState};
use crate::strategy::Strategy;
use crate::stats::StatsTracker; // <-- NEW
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct BacktestResults {
    pub trades: Vec<Trade>,
    pub metrics: BacktestMetrics,
    pub duration: Duration,
}

#[derive(Debug)]
pub struct BacktestMetrics {
    pub total_trades: usize,
    pub win_rate: f64,
    pub profit_factor: f64,
    pub total_profit: f64,
    pub max_drawdown: f64,
    pub sharpe_ratio: f64,
    pub sortino_ratio: f64,
    pub risk_reward_ratio: f64,
}

pub struct Backtester {
    initial_balance: f64,
    strategy: Strategy,
    trades: Vec<Trade>,
    equity_curve: Vec<f64>,
    current_balance: f64,
    state: BacktestState,
    stats: StatsTracker, // <-- NEW
}

impl Backtester {
    pub fn new(initial_balance: f64, strategy: Strategy) -> Self {
        let state = BacktestState {
            account_balance: initial_balance,
            initial_balance,
            position: None,
            equity_curve: vec![initial_balance],
            trades: Vec::new(),
            max_drawdown: 0.0,
            peak_balance: initial_balance,
            current_drawdown: 0.0,
        };

        Self {
            initial_balance,
            strategy,
            trades: Vec::new(),
            equity_curve: vec![initial_balance],
            current_balance: initial_balance,
            state,
            stats: StatsTracker::new(), // <-- NEW
        }
    }

    pub fn run(&mut self, candles: &[Candle]) -> Result<BacktestResults, Box<dyn std::error::Error>> {
        let start = Instant::now();

        for candle in candles {
            if let Some(trade) = self.strategy.analyze_candle(candle, &mut self.state) {
                self.execute_trade(candle, trade);
            }
            self.equity_curve.push(self.current_balance);
        }

        let metrics = self.calculate_metrics();

        Ok(BacktestResults {
            trades: self.trades.clone(),
            metrics,
            duration: start.elapsed(),
        })
    }

    fn execute_trade(&mut self, candle: &Candle, mut trade: Trade) {
        if trade.exit_price >= trade.entry_price && candle.high >= trade.exit_price {
            trade.pnl = (trade.exit_price - trade.entry_price) * trade.size;
        } else if trade.exit_price <= trade.entry_price && candle.low <= trade.exit_price {
            trade.pnl = (trade.entry_price - trade.exit_price) * trade.size;
        }

        trade.profit_factor = if trade.pnl > 0.0 {
            trade.pnl / (trade.size * trade.entry_price)
        } else {
            0.0
        };

        self.current_balance += trade.pnl;
        self.trades.push(trade.clone());

        self.stats.record_trade(&trade, self.current_balance); // <-- NEW
    }

    fn calculate_metrics(&self) -> BacktestMetrics {
        let total_trades = self.trades.len();
        let winning_trades = self.trades.iter().filter(|t| t.pnl > 0.0).count();
        let win_rate = if total_trades > 0 {
            winning_trades as f64 / total_trades as f64
        } else {
            0.0
        };

        let gross_profit: f64 = self.trades.iter().filter(|t| t.pnl > 0.0).map(|t| t.pnl).sum();
        let gross_loss: f64 = self.trades.iter().filter(|t| t.pnl < 0.0).map(|t| t.pnl.abs()).sum();
        let profit_factor = if gross_loss > 0.0 {
            gross_profit / gross_loss
        } else {
            f64::INFINITY
        };

        let total_profit: f64 = self.trades.iter().map(|t| t.pnl).sum();
        let max_drawdown = self.calculate_max_drawdown();
        let sharpe_ratio = self.calculate_sharpe_ratio();
        let sortino_ratio = self.calculate_sortino_ratio();
        let risk_reward_ratio = self.calculate_risk_reward_ratio();

        BacktestMetrics {
            total_trades,
            win_rate,
            profit_factor,
            total_profit,
            max_drawdown,
            sharpe_ratio,
            sortino_ratio,
            risk_reward_ratio,
        }
    }

    fn calculate_max_drawdown(&self) -> f64 {
        let mut peak: f64 = self.initial_balance;
        let mut max_drawdown: f64 = 0.0;

        for &balance in &self.equity_curve {
            peak = peak.max(balance);
            let drawdown = (peak - balance) / peak;
            max_drawdown = max_drawdown.max(drawdown);
        }

        max_drawdown
    }

    fn calculate_sharpe_ratio(&self) -> f64 {
        if self.trades.is_empty() {
            return 0.0;
        }

        let returns: Vec<f64> = self.trades.iter().map(|t| t.pnl / self.initial_balance).collect();
        let avg_return = returns.iter().sum::<f64>() / returns.len() as f64;
        let std_dev = (returns.iter().map(|r| (r - avg_return).powi(2)).sum::<f64>() / returns.len() as f64).sqrt();

        if std_dev == 0.0 {
            return 0.0;
        }

        avg_return / std_dev * (252.0_f64).sqrt()
    }

    fn calculate_sortino_ratio(&self) -> f64 {
        let downside_returns: Vec<f64> = self
            .trades
            .iter()
            .map(|t| t.pnl / self.initial_balance)
            .filter(|&r| r < 0.0)
            .collect();

        if downside_returns.is_empty() {
            return 0.0;
        }

        let avg_return = downside_returns.iter().sum::<f64>() / downside_returns.len() as f64;
        let downside_deviation = (downside_returns
            .iter()
            .map(|r| (r - avg_return).powi(2))
            .sum::<f64>()
            / downside_returns.len() as f64)
            .sqrt();

        if downside_deviation == 0.0 {
            return 0.0;
        }

        avg_return / downside_deviation
    }

    fn calculate_risk_reward_ratio(&self) -> f64 {
        let avg_win = self
            .trades
            .iter()
            .filter(|t| t.pnl > 0.0)
            .map(|t| t.pnl)
            .sum::<f64>()
            / self.trades.iter().filter(|t| t.pnl > 0.0).count().max(1) as f64;

        let avg_loss = self
            .trades
            .iter()
            .filter(|t| t.pnl < 0.0)
            .map(|t| t.pnl.abs())
            .sum::<f64>()
            / self.trades.iter().filter(|t| t.pnl < 0.0).count().max(1) as f64;

        if avg_loss == 0.0 {
            return 0.0;
        }

        avg_win / avg_loss
    }

    pub fn stats(&self) -> &StatsTracker {
        &self.stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Candle;
    use crate::strategy::StrategyConfig;

    #[test]
    fn test_backtester_run() {
        let candles = vec![
            Candle {
                time: "2023-01-01T00:00:00Z".to_string(),
                open: 100.0,
                high: 110.0,
                low: 90.0,
                close: 105.0,
                volume: 1000.0,
                num_trades: 50,
            },
            Candle {
                time: "2023-01-02T00:00:00Z".to_string(),
                open: 105.0,
                high: 115.0,
                low: 95.0,
                close: 100.0,
                volume: 1200.0,
                num_trades: 55,
            },
        ];

        let config = StrategyConfig::default();
        let strategy = Strategy::new(config);
        let mut backtester = Backtester::new(10000.0, strategy);

        let results = backtester.run(&candles).unwrap();
        assert!(results.trades.len() > 0);
        assert!(results.metrics.total_trades > 0);
    }
}
