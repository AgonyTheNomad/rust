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

        Some(FibLevels {
            entry_price: prev_high - self.initial_level * range,
            take_profit: prev_high + self.tp_level * range,
            stop_loss: prev_high - self.sl_level * range,
            limit1: prev_high - self.limit1_level * range,
            limit2: prev_high - self.limit2_level * range,
        })
    }

    pub fn calculate_short_levels(&self, prev_high: f64, prev_low: f64) -> Option<FibLevels> {
        let range = prev_high - prev_low;
        if range < self.threshold {
            return None;
        }

        Some(FibLevels {
            entry_price: prev_high + self.initial_level * range,
            take_profit: prev_low - self.tp_level * range,
            stop_loss: prev_high + self.sl_level * range,
            limit1: prev_high + self.limit1_level * range,
            limit2: prev_high + self.limit2_level * range,
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