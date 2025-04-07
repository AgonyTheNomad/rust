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
        
        // Add new values to the windows
        self.high_window.push_back(high);
        self.low_window.push_back(low);
        
        // Maintain the window size
        if self.high_window.len() > window_size {
            self.high_window.pop_front();
            self.low_window.pop_front();
        }

        // If we don't have a full window yet, return None
        if self.high_window.len() < window_size {
            return (None, None);
        }

        // The center index is the lookback value
        let center_idx = self.lookback;
        let center_high = self.high_window[center_idx];
        let center_low = self.low_window[center_idx];

        // Check if center point is the maximum in the window
        let pivot_high = if self.is_max_in_window(center_idx, &self.high_window) {
            Some(center_high)
        } else {
            None
        };

        // Check if center point is the minimum in the window
        let pivot_low = if self.is_min_in_window(center_idx, &self.low_window) {
            Some(center_low)
        } else {
            None
        };

        (pivot_high, pivot_low)
    }

    // Check if the value at center_idx is the maximum in the window
    fn is_max_in_window(&self, center_idx: usize, window: &VecDeque<f64>) -> bool {
        let center_value = window[center_idx];
        
        // Check all values in the window
        window.iter().enumerate().all(|(i, &value)| {
            i == center_idx || value <= center_value
        })
    }

    // Check if the value at center_idx is the minimum in the window
    fn is_min_in_window(&self, center_idx: usize, window: &VecDeque<f64>) -> bool {
        let center_value = window[center_idx];
        
        // Check all values in the window
        window.iter().enumerate().all(|(i, &value)| {
            i == center_idx || value >= center_value
        })
    }
    
    // Get the current size of the window
    pub fn window_size(&self) -> usize {
        self.high_window.len()
    }
    
    // Reset the pivot detector
    pub fn reset(&mut self) {
        self.high_window.clear();
        self.low_window.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pivot_identification() {
        let mut pp = PivotPoints::new(2);
        let data = vec![
            (10.0, 8.0),
            (11.0, 9.0),
            (12.0, 10.0),
            (11.0, 9.0),
            (10.0, 8.0),
        ];

        let mut pivots = Vec::new();
        for (high, low) in data {
            pivots.push(pp.identify_pivots(high, low));
        }

        // First two calls should return None as we don't have enough data
        assert_eq!(pivots[0], (None, None));
        assert_eq!(pivots[1], (None, None));
        
        // Third call should identify a pivot high at 12.0
        // because it's the highest in the window [10.0, 11.0, 12.0, 11.0, 10.0]
        assert!(pivots[2].0.is_some());
        assert_eq!(pivots[2].0.unwrap(), 12.0);
        
        // Check if we have a pivot low at 10.0 in position 2
        if let Some(low) = pivots[2].1 {
            assert_eq!(low, 10.0);
        }
    }

    #[test]
    fn test_multiple_pivot_sequences() {
        let mut pp = PivotPoints::new(1);
        
        // Test data designed to create clear pivot patterns
        let data = vec![
            (100.0, 90.0), // 0
            (105.0, 85.0), // 1
            (103.0, 82.0), // 2
            (108.0, 88.0), // 3
            (112.0, 92.0), // 4
            (107.0, 87.0), // 5
        ];

        let mut pivots = Vec::new();
        for (high, low) in data {
            pivots.push(pp.identify_pivots(high, low));
        }

        // We need at least 2*lookback+1 points, so first 2 should be None
        assert_eq!(pivots[0], (None, None));
        assert_eq!(pivots[1], (None, None));
        
        // Position 2 should identify a pivot low
        assert!(pivots[2].1.is_some());
        assert_eq!(pivots[2].1.unwrap(), 82.0);
        
        // Position 4 should identify a pivot high
        assert!(pivots[4].0.is_some());
        assert_eq!(pivots[4].0.unwrap(), 112.0);
    }
    
    #[test]
    fn test_reset_functionality() {
        let mut pp = PivotPoints::new(2);
        
        // Add some data
        for _ in 0..5 {
            pp.identify_pivots(100.0, 90.0);
        }
        
        // Verify we have a full window
        assert_eq!(pp.window_size(), 5);
        
        // Reset and verify it's empty
        pp.reset();
        assert_eq!(pp.window_size(), 0);
        
        // Add new data after reset
        pp.identify_pivots(105.0, 95.0);
        assert_eq!(pp.window_size(), 1);
    }
}