use std::collections::VecDeque;

#[derive(Debug)]
pub struct PivotPoints {
    lookback: usize,
    high_window: VecDeque<f64>,
    low_window: VecDeque<f64>,
}

impl PivotPoints {
    pub fn new(lookback: usize) -> Self {
        Self {
            lookback,
            high_window: VecDeque::with_capacity(lookback * 2 + 1),
            low_window: VecDeque::with_capacity(lookback * 2 + 1),
        }
    }

    pub fn identify_pivots(&mut self, high: f64, low: f64) -> (Option<f64>, Option<f64>) {
        let window_size = self.lookback * 2 + 1;
        self.high_window.push_back(high);
        self.low_window.push_back(low);

        if self.high_window.len() > window_size {
            self.high_window.pop_front();
            self.low_window.pop_front();
        }

        if self.high_window.len() < window_size {
            return (None, None);
        }

        let center_idx = self.lookback;
        let center_high = self.high_window[center_idx];
        let center_low = self.low_window[center_idx];

        let pivot_high = if self.is_max_in_window(center_idx, &self.high_window) {
            Some(center_high)
        } else {
            None
        };

        let pivot_low = if self.is_min_in_window(center_idx, &self.low_window) {
            Some(center_low)
        } else {
            None
        };

        (pivot_high, pivot_low)
    }

    fn is_max_in_window(&self, center_idx: usize, window: &VecDeque<f64>) -> bool {
        let center_value = window[center_idx];
        window.iter().enumerate().all(|(i, &value)| i == center_idx || value <= center_value)
    }

    fn is_min_in_window(&self, center_idx: usize, window: &VecDeque<f64>) -> bool {
        let center_value = window[center_idx];
        window.iter().enumerate().all(|(i, &value)| i == center_idx || value >= center_value)
    }

    pub fn window_size(&self) -> usize {
        self.high_window.len()
    }

    pub fn reset(&mut self) {
        self.high_window.clear();
        self.low_window.clear();
    }
}
