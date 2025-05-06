use serde::{Deserialize, Serialize};
use std::{fs::File, io::Write};
use chrono::{DateTime, Utc};
use crate::models::Trade;

#[derive(Debug, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub total_trades: usize,
    pub winning_trades: usize,
    pub losing_trades: usize,
    pub total_profit: f64,
    pub total_return_percent: f64,
    pub annualized_return: f64,
    pub largest_win: f64,
    pub largest_loss: f64,
    pub average_win: f64,
    pub average_loss: f64,
    pub win_rate: f64,
    pub loss_rate: f64,
    pub profit_factor: f64,
    pub max_drawdown: f64,
    pub max_drawdown_duration: i64,
    pub sharpe_ratio: f64,
    pub sortino_ratio: f64,
    pub calmar_ratio: f64,
    pub value_at_risk: f64,
    pub conditional_var: f64,
    pub average_position_size: f64,
    pub max_position_size: f64,
    pub average_leverage: f64,
    pub max_leverage: f64,
    pub time_in_market: f64,
    pub total_fees: f64,
    pub total_slippage: f64,
    pub risk_reward_ratio: f64,
    pub average_trade_duration: i64,
}

pub struct MetricsCalculator {
    initial_balance: f64,
    risk_free_rate: f64,
    trades: Vec<Trade>,
    equity_curve: Vec<f64>,
    timestamps: Vec<DateTime<Utc>>,
}

impl MetricsCalculator {
    pub fn new(initial_balance: f64, risk_free_rate: f64) -> Self {
        Self {
            initial_balance,
            risk_free_rate,
            trades: Vec::new(),
            equity_curve: vec![initial_balance],
            timestamps: Vec::new(),
        }
    }

    pub fn add_trade(&mut self, trade: Trade) {
        self.trades.push(trade);
    }

    pub fn update_equity(&mut self, value: f64, timestamp: DateTime<Utc>) {
        self.equity_curve.push(value);
        self.timestamps.push(timestamp);
    }

    pub fn calculate(&self) -> PerformanceMetrics {
        let mut winning_trades = 0;
        let mut losing_trades = 0;
        let mut total_profit = 0.0;
        let mut largest_win: f64 = 0.0;  // Explicitly type as f64
        let mut largest_loss: f64 = 0.0;  // Explicitly type as f64
        let mut total_wins = 0.0;
        let mut total_losses = 0.0;
        let mut total_leverage: f64 = 0.0;  // Explicitly type as f64
        let mut max_leverage: f64 = 0.0;  // Explicitly type as f64
        let mut total_position_size = 0.0;
        let mut max_position_size: f64 = 0.0;  // Explicitly type as f64
        let mut total_fees = 0.000144;
        let mut total_slippage = 0.0;
        let total_duration = 0i64;

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
            total_leverage += trade.margin_used;
            max_leverage = max_leverage.max(trade.margin_used);
            total_position_size += trade.size;
            max_position_size = max_position_size.max(trade.size);
            total_fees += trade.fees;
            total_slippage += trade.slippage;
        }

        let total_trades = self.trades.len();
        let win_rate = if total_trades > 0 {
            winning_trades as f64 / total_trades as f64
        } else {
            0.0
        };

        let loss_rate = 1.0 - win_rate;
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

        let profit_factor = if total_losses > 0.0 {
            total_wins / total_losses
        } else {
            f64::INFINITY
        };

        // Calculate returns and risk metrics
        let returns = self.calculate_returns();
        let (sharpe_ratio, sortino_ratio) = self.calculate_risk_adjusted_returns(&returns);
        let (max_drawdown, max_drawdown_duration) = self.calculate_drawdown();
        let (var, cvar) = self.calculate_var_metrics(&returns);
        let time_in_market = self.calculate_time_in_market();
        let risk_reward_ratio = if average_loss != 0.0 {
            average_win.abs() / average_loss
        } else {
            f64::INFINITY
        };

        PerformanceMetrics {
            total_trades,
            winning_trades,
            losing_trades,
            total_profit,
            total_return_percent: (total_profit / self.initial_balance) * 100.0,
            annualized_return: self.calculate_annualized_return(total_profit),
            largest_win,
            largest_loss,
            average_win,
            average_loss,
            win_rate,
            loss_rate,
            profit_factor,
            max_drawdown,
            max_drawdown_duration,
            sharpe_ratio,
            sortino_ratio,
            calmar_ratio: self.calculate_calmar_ratio(total_profit, max_drawdown),
            value_at_risk: var,
            conditional_var: cvar,
            average_position_size: total_position_size / total_trades as f64,
            max_position_size,
            average_leverage: total_leverage / total_trades as f64,
            max_leverage,
            time_in_market,
            total_fees,
            total_slippage,
            risk_reward_ratio,
            average_trade_duration: if total_trades > 0 {
                total_duration / total_trades as i64
            } else {
                0
            },
        }
    }

    fn calculate_returns(&self) -> Vec<f64> {
        self.equity_curve
            .windows(2)
            .map(|w| (w[1] - w[0]) / w[0])
            .collect()
    }

    fn calculate_risk_adjusted_returns(&self, returns: &[f64]) -> (f64, f64) {
        if returns.is_empty() {
            return (0.0, 0.0);
        }

        let avg_return = returns.iter().sum::<f64>() / returns.len() as f64;
        let variance = returns.iter()
            .map(|r| (r - avg_return).powi(2))
            .sum::<f64>() / returns.len() as f64;
        let std_dev = variance.sqrt();

        let negative_returns: Vec<f64> = returns.iter()
            .filter(|&&r| r < 0.0)
            .copied()
            .collect();
        
        let downside_std_dev = if !negative_returns.is_empty() {
            let avg_negative = negative_returns.iter().sum::<f64>() / negative_returns.len() as f64;
            (negative_returns.iter()
                .map(|r| (r - avg_negative).powi(2))
                .sum::<f64>() / negative_returns.len() as f64)
                .sqrt()
        } else {
            0.0
        };

        let sharpe_ratio = if std_dev > 0.0 {
            (avg_return - self.risk_free_rate) / std_dev * (252.0_f64).sqrt()
        } else {
            0.0
        };

        let sortino_ratio = if downside_std_dev > 0.0 {
            (avg_return - self.risk_free_rate) / downside_std_dev * (252.0_f64).sqrt()
        } else {
            0.0
        };

        (sharpe_ratio, sortino_ratio)
    }

    fn calculate_drawdown(&self) -> (f64, i64) {
        let mut max_drawdown: f64 = 0.0;
        let mut current_peak = self.equity_curve[0];
        let mut drawdown_start = 0;
        let mut max_drawdown_duration = 0;

        for (i, &value) in self.equity_curve.iter().enumerate() {
            if value > current_peak {
                current_peak = value;
                drawdown_start = i;
            }

            let drawdown = (current_peak - value) / current_peak;
            if drawdown > max_drawdown {
                max_drawdown = drawdown;
                if i > drawdown_start {
                    max_drawdown_duration = (self.timestamps[i] - self.timestamps[drawdown_start]).num_hours();
                }
            }
        }

        (max_drawdown, max_drawdown_duration)
    }

    fn calculate_var_metrics(&self, returns: &[f64]) -> (f64, f64) {
        if returns.is_empty() {
            return (0.0, 0.0);
        }

        let mut sorted_returns = returns.to_vec();
        sorted_returns.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let confidence_level = 0.95;
        let var_index = ((1.0 - confidence_level) * returns.len() as f64) as usize;
        let var = sorted_returns[var_index];

        let cvar = sorted_returns[..var_index]
            .iter()
            .sum::<f64>() / var_index as f64;

        (var, cvar)
    }

    fn calculate_time_in_market(&self) -> f64 {
        if self.timestamps.len() < 2 {
            return 0.0;
        }

        // Use * to dereference the DateTime references
        let total_time = (*self.timestamps.last().unwrap() - *self.timestamps.first().unwrap()).num_seconds() as f64;
        
        let time_in_trades = self.trades.iter()
            .map(|t| {
                // Parse strings to DateTime to calculate duration
                let exit_time = chrono::DateTime::parse_from_rfc3339(&t.exit_time)
                    .unwrap_or(Utc::now().into())
                    .with_timezone(&Utc);
                let entry_time = chrono::DateTime::parse_from_rfc3339(&t.entry_time)
                    .unwrap_or(Utc::now().into())
                    .with_timezone(&Utc);
                (exit_time - entry_time).num_seconds() as f64
            })
            .sum::<f64>();

        time_in_trades / total_time
    }

    fn calculate_annualized_return(&self, total_profit: f64) -> f64 {
        if self.timestamps.len() < 2 {
            return 0.0;
        }

        // Use * to dereference the DateTime references
        let years = (*self.timestamps.last().unwrap() - *self.timestamps.first().unwrap()).num_days() as f64 / 365.0;
        if years > 0.0 {
            ((1.0 + total_profit / self.initial_balance).powf(1.0 / years) - 1.0) * 100.0
        } else {
            0.0
        }
    }

    fn calculate_calmar_ratio(&self, total_profit: f64, max_drawdown: f64) -> f64 {
        if max_drawdown == 0.0 {
            return 0.0;
        }
        
        let annualized_return = self.calculate_annualized_return(total_profit);
        annualized_return / (max_drawdown * 100.0)
    }

    pub fn generate_report(&self) -> String {
        let metrics = self.calculate();
        // Your existing report generation code...
        format!(
            "Performance Report\n\
            ==================\n\
            Total Return: {:.2}%\n\
            Annualized Return: {:.2}%\n\
            Sharpe Ratio: {:.2}\n\
            Sortino Ratio: {:.2}\n\
            Maximum Drawdown: {:.2}%\n\
            Calmar Ratio: {:.2}\n\
            \n\
            Trade Statistics\n\
            ----------------\n\
            Total Trades: {}\n\
            Win Rate: {:.2}%\n\
            Profit Factor: {:.2}\n\
            Average Win: ${:.2}\n\
            Average Loss: ${:.2}\n\
            Largest Win: ${:.2}\n\
            Largest Loss: ${:.2}\n\
            \n\
            Risk Metrics\n\
            ------------\n\
            Value at Risk (95%): {:.2}%\n\
            Conditional VaR (95%): {:.2}%\n\
            Average Leverage: {:.2}x\n\
            Maximum Leverage: {:.2}x\n\
            \n\
            Trading Metrics\n\
            --------------\n\
            Average Position Size: {:.4}\n\
            Max Position Size: {:.4}\n\
            Average Trade Duration: {} hours\n\
            Time in Market: {:.2}%\n\
            \n\
            Cost Analysis\n\
            -------------\n\
            Total Fees: ${:.2}\n\
            Total Slippage: ${:.2}\n\
            Total Trading Costs: ${:.2}",
            metrics.total_return_percent,
            metrics.annualized_return,
            metrics.sharpe_ratio,
            metrics.sortino_ratio,
            metrics.max_drawdown * 100.0,
            metrics.calmar_ratio,
            metrics.total_trades,
            metrics.win_rate * 100.0,
            metrics.profit_factor,
            metrics.average_win,
            metrics.average_loss,
            metrics.largest_win,
            metrics.largest_loss,
            metrics.value_at_risk * 100.0,
            metrics.conditional_var * 100.0,
            metrics.average_leverage,
            metrics.max_leverage,
            metrics.average_position_size,
            metrics.max_position_size,
            metrics.average_trade_duration,
            metrics.time_in_market * 100.0,
            metrics.total_fees,
            metrics.total_slippage,
            metrics.total_fees + metrics.total_slippage
        )
    }

    pub fn save_report(&self, filepath: &str) -> std::io::Result<()> {
        let report = self.generate_report();
        let mut file = File::create(filepath)?;
        file.write_all(report.as_bytes())?;
        Ok(())
    }

    pub fn generate_json_report(&self) -> String {
        let metrics = self.calculate();
        serde_json::to_string_pretty(&metrics)
            .unwrap_or_else(|_| "Error generating JSON report".to_string())
    }

    pub fn save_json_report(&self, filepath: &str) -> std::io::Result<()> {
        let report = self.generate_json_report();
        let mut file = File::create(filepath)?;
        file.write_all(report.as_bytes())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    #[test]
    fn test_metrics_calculation() {
        let mut calculator = MetricsCalculator::new(10000.0, 0.02);  // Initial balance and risk-free rate
        
        // Add some sample trades
        let trades = vec![
            Trade {
                entry_time: "2024-01-01T00:00:00Z".to_string(),
                exit_time: "2024-01-02T00:00:00Z".to_string(),
                position_type: "Long".to_string(),
                entry_price: 100.0,
                exit_price: 110.0,
                size: 1.0,
                pnl: 10.0,
                risk_percent: 1.0,
                profit_factor: 2.0,
                margin_used: 5.0,
                fees: 0.000144,
                slippage: 0.0,
            },
            Trade {
                entry_time: "2024-01-03T00:00:00Z".to_string(),
                exit_time: "2024-01-04T00:00:00Z".to_string(),
                position_type: "Short".to_string(),
                entry_price: 110.0,
                exit_price: 100.0,
                size: 1.0,
                pnl: -10.0,
                risk_percent: 1.0,
                profit_factor: 0.0,
                margin_used: 5.0,
                fees: 0.000144,
                slippage: 0.0,
            },
        ];

        for trade in trades {
            calculator.add_trade(trade.clone());
            let exit_time = chrono::DateTime::parse_from_rfc3339(&trade.exit_time)
                .unwrap()
                .with_timezone(&Utc);
            calculator.update_equity(
                calculator.initial_balance + calculator.trades.last().unwrap().pnl,
                exit_time,
            );
        }

        let metrics = calculator.calculate();

        // Basic metrics tests
        assert_eq!(metrics.total_trades, 2);
        assert_eq!(metrics.winning_trades, 1);
        assert_eq!(metrics.losing_trades, 1);
        
        // Performance metrics tests
        assert!(metrics.win_rate == 0.5);
        assert!(metrics.total_profit == 0.0);  // One winning and one losing trade of equal size
        assert!(metrics.profit_factor == 1.0);  // Equal wins and losses
        
        // Risk metrics tests
        assert!(metrics.max_drawdown >= 0.0);
        assert!(metrics.sharpe_ratio.is_finite());
        assert!(metrics.sortino_ratio.is_finite());
    }

    #[test]
    fn test_report_generation() {
        let mut calculator = MetricsCalculator::new(10000.0, 0.02);
        
        // Add a sample trade
        calculator.add_trade(Trade {
            entry_time: "2024-01-01T00:00:00Z".to_string(),
            exit_time: "2024-01-02T00:00:00Z".to_string(),
            position_type: "Long".to_string(),
            entry_price: 100.0,
            exit_price: 110.0,
            size: 1.0,
            pnl: 10.0,
            risk_percent: 1.0,
            profit_factor: 2.0,
            margin_used: 5.0,
            fees: 0.000144,
            slippage: 0.0,
        });

        calculator.update_equity(10010.0, Utc.ymd(2024, 1, 2).and_hms(0, 0, 0));
        
        let report = calculator.generate_report();
        assert!(report.contains("Performance Report"));
        assert!(report.contains("Trade Statistics"));
        assert!(report.contains("Risk Metrics"));
    }

    #[test]
    fn test_drawdown_calculation() {
        let mut calculator = MetricsCalculator::new(10000.0, 0.02);
        
        // Simulate equity curve with known drawdown
        let timestamps = vec![
            Utc.ymd(2024, 1, 1).and_hms(0, 0, 0),
            Utc.ymd(2024, 1, 2).and_hms(0, 0, 0),
            Utc.ymd(2024, 1, 3).and_hms(0, 0, 0),
            Utc.ymd(2024, 1, 4).and_hms(0, 0, 0),
        ];

        let equity_values = vec![10000.0, 11000.0, 9500.0, 10500.0];
        
        for (i, value) in equity_values.iter().enumerate() {
            calculator.update_equity(*value, timestamps[i]);
        }

        let metrics = calculator.calculate();
        assert!(metrics.max_drawdown > 0.0);  // Should detect the drawdown
        assert!(metrics.max_drawdown < 1.0);  // Drawdown should be less than 100%
    }

    #[test]
    fn test_risk_metrics() {
        let mut calculator = MetricsCalculator::new(10000.0, 0.02);
        
        // Add trades with known risk/reward characteristics
        let trades = vec![
            Trade {
                entry_time: "2024-01-01T00:00:00Z".to_string(),
                exit_time: "2024-01-02T00:00:00Z".to_string(),
                position_type: "Long".to_string(),
                entry_price: 100.0,
                exit_price: 120.0,
                size: 1.0,
                pnl: 20.0,
                risk_percent: 1.0,
                profit_factor: 2.0,
                margin_used: 5.0,
                fees: 0.000144,
                slippage: 0.0,
            },
            Trade {
                entry_time: "2024-01-03T00:00:00Z".to_string(),
                exit_time: "2024-01-04T00:00:00Z".to_string(),
                position_type: "Long".to_string(),
                entry_price: 120.0,
                exit_price: 110.0,
                size: 1.0,
                pnl: -10.0,
                risk_percent: 1.0,
                profit_factor: 0.0,
                margin_used: 5.0,
                fees: 0.000144,
                slippage: 0.0,
            },
        ];

        for trade in trades {
            calculator.add_trade(trade.clone());
            let exit_time = chrono::DateTime::parse_from_rfc3339(&trade.exit_time)
                .unwrap()
                .with_timezone(&Utc);
            calculator.update_equity(
                calculator.initial_balance + calculator.trades.last().unwrap().pnl,
                exit_time,
            );
        }

        let metrics = calculator.calculate();
        
        // Test risk metrics
        assert!(metrics.sharpe_ratio > 0.0);  // Should be positive with winning trades
        assert!(metrics.value_at_risk < 0.0);  // VaR should be negative
        assert!(metrics.risk_reward_ratio > 1.0);  // Winning trades are larger than losing trades
    }

    #[test]
    fn test_json_report() {
        let calculator = MetricsCalculator::new(10000.0, 0.02);
        let json_report = calculator.generate_json_report();
        
        assert!(json_report.contains("total_trades"));
        assert!(json_report.contains("win_rate"));
        assert!(json_report.contains("sharpe_ratio"));
        
        // Verify JSON is valid
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&json_report);
        assert!(parsed.is_ok());
    }
}