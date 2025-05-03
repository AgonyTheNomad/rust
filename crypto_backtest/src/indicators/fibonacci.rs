#[derive(Debug)]
pub struct FibonacciLevels {
    pub threshold: f64,
    pub initial_level: f64,
    pub tp_level: f64,
    pub sl_level: f64,
    pub limit1_level: f64,
    pub limit2_level: f64,
}

#[allow(dead_code)]
impl FibonacciLevels {
    pub fn new(
        threshold: f64,
        initial_level: f64,
        tp_level: f64,
        sl_level: f64,
        limit1_level: f64,
        limit2_level: f64,
    ) -> Self {
        Self {
            threshold,
            initial_level,
            tp_level,
            sl_level,
            limit1_level,
            limit2_level,
        }
    }

    pub fn calculate_long_levels(&self, prev_high: f64, prev_low: f64) -> Option<FibLevels> {
        let range = prev_high - prev_low;
        if range < self.threshold {
            return None;
        }

        // Calculate the price levels
        let entry_price = prev_low + self.initial_level * range;
        let take_profit = prev_high + self.tp_level * range;
        let stop_loss = prev_low - self.sl_level * range;
        let limit1 = prev_low - self.limit1_level * range;
        let limit2 = prev_low - self.limit2_level * range;
        
        // Add debug output
        println!("LONG POSITION LEVELS:");
        println!("  Price Range: {} (from {} to {})", range, prev_low, prev_high);
        println!("  Entry: {:.2}", entry_price);
        println!("  Take Profit: {:.2}", take_profit);
        println!("  Stop Loss: {:.2}", stop_loss);
        println!("  Limit1: {:.2}", limit1);
        println!("  Limit2: {:.2}", limit2);
        
        // Validate the order of price levels
        if !(stop_loss < limit2 && limit2 < limit1 && limit1 < entry_price && entry_price < take_profit) {
            println!("WARNING: Invalid price order for long position!");
        }

        Some(FibLevels {
            entry_price,
            take_profit,
            stop_loss,
            limit1,
            limit2,
        })
    }

    pub fn calculate_short_levels(&self, prev_high: f64, prev_low: f64) -> Option<FibLevels> {
        let range = prev_high - prev_low;
        if range < self.threshold {
            return None;
        }

        // Calculate the price levels
        let entry_price = prev_high - self.initial_level * range;
        let take_profit = prev_low - self.tp_level * range;
        let stop_loss = prev_high + self.sl_level * range;
        let limit1 = prev_high + self.limit1_level * range;
        let limit2 = prev_high + self.limit2_level * range;
        
        // Add debug output
        println!("SHORT POSITION LEVELS:");
        println!("  Price Range: {} (from {} to {})", range, prev_low, prev_high);
        println!("  Entry: {:.2}", entry_price);
        println!("  Take Profit: {:.2}", take_profit);
        println!("  Stop Loss: {:.2}", stop_loss);
        println!("  Limit1: {:.2}", limit1);
        println!("  Limit2: {:.2}", limit2);
        
        // Validate the order of price levels
        if !(stop_loss > limit2 && limit2 > limit1 && limit1 > entry_price && entry_price > take_profit) {
            println!("WARNING: Invalid price order for short position!");
        }

        Some(FibLevels {
            entry_price,
            take_profit,
            stop_loss,
            limit1,
            limit2,
        })
    }
}

#[derive(Debug)]
pub struct FibLevels {
    pub entry_price: f64,
    pub take_profit: f64,
    pub stop_loss: f64,
    #[allow(dead_code)]
    pub limit1: f64,
    #[allow(dead_code)]
    pub limit2: f64,
}