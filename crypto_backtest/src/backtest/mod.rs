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
        // Transaction costs (could be moved to configuration)
        let transaction_cost = 0.0001; // 0.01% fee
        let stop_loss_cost = 0.00035;  // Higher cost for stop loss (slippage)
        
        // Determine if this is a take profit or stop loss exit
        let is_take_profit = match trade.position_type.as_str() {
            "Long" => trade.exit_price >= trade.entry_price,
            "Short" => trade.exit_price <= trade.entry_price,
            _ => false,
        };
        
        // Calculate raw P&L
        let raw_pnl = if trade.position_type == "Long" {
            (trade.exit_price - trade.entry_price) * trade.size
        } else { // Short
            (trade.entry_price - trade.exit_price) * trade.size
        };
        
        // Apply appropriate transaction costs
        let fee_cost = transaction_cost * trade.size * (trade.entry_price + trade.exit_price);
        let slippage_cost = if is_take_profit {
            0.0 // No extra slippage on take profit
        } else {
            stop_loss_cost * trade.size * trade.exit_price // Extra slippage on stop loss
        };
        
        // Calculate final P&L
        let final_pnl = raw_pnl - fee_cost - slippage_cost;
        
        // Update trade information
        trade.pnl = final_pnl;
        trade.fees = fee_cost;
        trade.slippage = slippage_cost;
        
        // Update profit factor if applicable
        trade.profit_factor = if final_pnl > 0.0 {
            final_pnl / (trade.size * trade.entry_price)
        } else {
            0.0
        };
    
        self.current_balance += final_pnl;
        self.trades.push(trade.clone());
    
        // Record trade with the stats tracker
        self.stats.record_trade(&trade, self.current_balance);
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
        // Create a more sophisticated test dataset that will trigger trades
        let mut candles = Vec::new();
        
        // Generate candles with a clear trend and reversal pattern
        // Initial uptrend
        for i in 0..8 {
            let base = 100.0 + (i as f64 * 10.0);
            candles.push(Candle {
                time: format!("2023-01-{:02}T00:00:00Z", i+1),
                open: base,
                high: base + 5.0 + (i as f64 * 2.0), // Increasing highs
                low: base - 2.0,
                close: base + 4.0,
                volume: 1000.0 + (i as f64 * 100.0),
                num_trades: 50 + (i * 5),
            });
        }
        
        // Clear peak and reversal
        candles.push(Candle {
            time: "2023-01-09T00:00:00Z".to_string(),
            open: 180.0,
            high: 195.0, // Strong high - this should create a pivot high
            low: 175.0,
            close: 176.0, // Close lower, suggesting reversal
            volume: 2500.0, // Higher volume on reversal
            num_trades: 90,
        });
        
        // Downtrend
        for i in 0..6 {
            let base = 175.0 - (i as f64 * 8.0);
            candles.push(Candle {
                time: format!("2023-01-{:02}T00:00:00Z", i+10),
                open: base,
                high: base + 3.0,
                low: base - 5.0 - (i as f64 * 1.5), // Decreasing lows
                close: base - 4.0,
                volume: 1800.0 - (i as f64 * 100.0),
                num_trades: 70 - (i * 3),
            });
        }
        
        // Bottom formation and reversal
        candles.push(Candle {
            time: "2023-01-16T00:00:00Z".to_string(),
            open: 120.0,
            high: 124.0,
            low: 105.0, // Strong low - this should create a pivot low
            close: 122.0, // Close higher, suggesting reversal
            volume: 2700.0, // Higher volume on reversal
            num_trades: 95,
        });
        
        // New uptrend
        for i in 0..5 {
            let base = 125.0 + (i as f64 * 7.0);
            candles.push(Candle {
                time: format!("2023-01-{:02}T00:00:00Z", i+17),
                open: base - 2.0,
                high: base + 5.0,
                low: base - 3.0,
                close: base + 4.0,
                volume: 1600.0 + (i as f64 * 150.0),
                num_trades: 65 + (i * 4),
            });
        }

        // Configure a strategy that will work with our test data
        let config = StrategyConfig {
            initial_balance: 10000.0,
            leverage: 10.0,
            max_risk_per_trade: 0.02,      // Higher risk for test
            pivot_lookback: 2,             // Small lookback for test
            signal_lookback: 1,            // Quick signal generation
            fib_threshold: 5.0,            // Lower threshold to ensure entry
            fib_initial: 0.382,            // Standard Fibonacci entry
            fib_tp: 0.618,                 // Standard take profit
            fib_sl: 0.236,                 // Standard stop loss
            ..Default::default()
        };
        
        let strategy = Strategy::new(config);
        let mut backtester = Backtester::new(10000.0, strategy);

        let results = backtester.run(&candles).unwrap();
        
        // Debug output to see what happened
        println!("Test trade count: {}", results.trades.len());
        if !results.trades.is_empty() {
            println!("First trade: {} {} Entry: {}, Exit: {}, PnL: {}", 
                results.trades[0].position_type,
                results.trades[0].entry_time,
                results.trades[0].entry_price,
                results.trades[0].exit_price,
                results.trades[0].pnl);
        }
        
        // Assertions
        assert!(results.trades.len() > 0, "No trades were executed!");
        assert!(results.metrics.total_trades > 0);
        assert!(results.metrics.win_rate >= 0.0 && results.metrics.win_rate <= 1.0);
    }
}