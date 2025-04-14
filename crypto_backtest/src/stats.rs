use crate::models::Trade;
use std::collections::HashMap;


#[derive(Debug, Default)]
pub struct StatsTracker {
    pub wins: u32,
    pub losses: u32,
    pub total_trades: u32,
    pub total_pnl: f64,
    pub equity_curve: HashMap<String, f64>, // timestamp → balance
}

impl StatsTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_trade(&mut self, trade: &Trade, account_balance: f64) {
        self.total_trades += 1;
        self.total_pnl += trade.pnl;

        if trade.pnl > 0.0 {
            self.wins += 1;
        } else {
            self.losses += 1;
        }

        // Store equity snapshot by date
        let timestamp = trade.exit_time.clone(); // You could trim to date if you want
        self.equity_curve.insert(timestamp, account_balance);
    }

    pub fn win_rate(&self) -> f64 {
        if self.total_trades == 0 {
            return 0.0;
        }
        self.wins as f64 / self.total_trades as f64
    }

    pub fn average_pnl(&self) -> f64 {
        if self.total_trades == 0 {
            return 0.0;
        }
        self.total_pnl / self.total_trades as f64
    }
}
