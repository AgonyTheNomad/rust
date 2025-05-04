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

        // Calculate main price levels
        let entry_price = prev_low + self.initial_level * range;
        let take_profit = prev_high + self.tp_level * range;
        let stop_loss = prev_low - self.sl_level * range;
        
        // For a proper scale-in strategy: 
        // Limit orders should be between entry price and stop loss
        // Calculate how far entry is from stop loss
        let entry_to_sl_distance = entry_price - stop_loss;
        
        // Place Limit1 at 1/3 of the way from entry to stop loss
        let limit1 = entry_price - (entry_to_sl_distance * 0.33);
        
        // Place Limit2 at 2/3 of the way from entry to stop loss
        let limit2 = entry_price - (entry_to_sl_distance * 0.66);
        
        // This ensures: SL < Limit2 < Limit1 < Entry < TP

       // println!("LONG POSITION LEVELS:");
        //println!("  Price Range: {:.2} ({}→{})", range, prev_low, prev_high);
        //println!("  Entry:       {:.2}", entry_price);
        //println!("  Take Profit: {:.2}", take_profit);
        //println!("  Stop Loss:   {:.2}", stop_loss);
        //println!("  Limit1:      {:.2}", limit1);
        //println!("  Limit2:      {:.2}", limit2);

        // Validate the ordering
        if !(stop_loss < limit2 
           && limit2 < limit1 
           && limit1 < entry_price 
           && entry_price < take_profit)
        {
            //println!("WARNING: Invalid price order for long position!");
            return None; // Return None if levels are invalid
        }

        Some(FibLevels { entry_price, take_profit, stop_loss, limit1, limit2 })
    }

    pub fn calculate_short_levels(&self, prev_high: f64, prev_low: f64) -> Option<FibLevels> {
        let range = prev_high - prev_low;
        if range < self.threshold {
            return None;
        }

        // Calculate main price levels
        let entry_price = prev_high - self.initial_level * range;
        let take_profit = prev_low - self.tp_level * range;
        let stop_loss = prev_high + self.sl_level * range;
        
        // For a proper scale-in strategy:
        // Limit orders should be between entry price and stop loss
        let entry_to_sl_distance = stop_loss - entry_price;
        
        // Place Limit1 at 1/3 of the way from entry to stop loss
        let limit1 = entry_price + (entry_to_sl_distance * 0.33);
        
        // Place Limit2 at 2/3 of the way from entry to stop loss
        let limit2 = entry_price + (entry_to_sl_distance * 0.66);
        
        // This ensures: TP < Entry < Limit1 < Limit2 < SL

        //println!("SHORT POSITION LEVELS:");
        //println!("  Price Range: {:.2} ({}→{})", range, prev_low, prev_high);
        //println!("  Entry:       {:.2}", entry_price);
        //println!("  Take Profit: {:.2}", take_profit);
        //println!("  Stop Loss:   {:.2}", stop_loss);
       // println!("  Limit1:      {:.2}", limit1);
        //println!("  Limit2:      {:.2}", limit2);

        // Validate the ordering
        if !(take_profit < entry_price
           && entry_price < limit1
           && limit1 < limit2
           && limit2 < stop_loss)
        {
           // println!("WARNING: Invalid price order for short position!");
            return None; // Return None if levels are invalid
        }

        Some(FibLevels { entry_price, take_profit, stop_loss, limit1, limit2 })
    }
    
    // Helper method to validate if a set of levels is valid for a long position
    pub fn validate_long_levels(&self, entry: f64, tp: f64, sl: f64, limit1: f64, limit2: f64) -> bool {
        sl < limit2 && limit2 < limit1 && limit1 < entry && entry < tp
    }
    
    // Helper method to validate if a set of levels is valid for a short position
    pub fn validate_short_levels(&self, entry: f64, tp: f64, sl: f64, limit1: f64, limit2: f64) -> bool {
        tp < entry && entry < limit1 && limit1 < limit2 && limit2 < sl
    }
}

#[derive(Debug)]
pub struct FibLevels {
    pub entry_price: f64,
    pub take_profit: f64,
    pub stop_loss: f64,
    pub limit1: f64,
    pub limit2: f64,
}