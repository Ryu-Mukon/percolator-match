//! Volatility Estimator
//!
//! Simple rolling window volatility calculation for dynamic spread adjustment.

use super::types::*;

/// Number of price observations to keep
const WINDOW_SIZE: usize = 24;

/// Simple volatility estimator using rolling window of prices
#[derive(Clone, Debug)]
pub struct VolatilityEstimator {
    /// Price history (most recent last)
    prices: [Price; WINDOW_SIZE],
    /// Current count of valid prices
    count: usize,
    /// Index for circular buffer
    head: usize,
    /// Last recorded price (for change calculation)
    last_price: Price,
    /// Whether we have enough data
    ready: bool,
}

impl VolatilityEstimator {
    /// Create a new volatility estimator
    pub fn new() -> Self {
        Self {
            prices: [0; WINDOW_SIZE],
            count: 0,
            head: 0,
            last_price: 0,
            ready: false,
        }
    }

    /// Add a new price observation
    pub fn update(&mut self, price: Price) {
        if price == 0 {
            return;
        }

        if self.count < WINDOW_SIZE {
            self.prices[self.count] = price;
            self.count += 1;
        } else {
            // Circular buffer overwrite
            self.prices[self.head] = price;
            self.head = (self.head + 1) % WINDOW_SIZE;
        }

        self.last_price = price;

        // Need at least 2 prices to compute volatility
        if self.count >= 2 {
            self.ready = true;
        }
    }

    /// Get current price
    pub fn current_price(&self) -> Price {
        self.last_price
    }

    /// Check if we have enough data
    pub fn is_ready(&self) -> bool {
        self.ready
    }

    /// Calculate volatility as basis points (annualized daily range)
    /// 
    /// Uses simplified calculation: (max - min) / mean * 10000
    /// Returns 0 if not enough data
    pub fn volatility_bps(&self) -> u32 {
        if !self.ready || self.count < 2 {
            return 0;
        }

        // Find min and max in current window
        let window_end = if self.count < WINDOW_SIZE {
            self.count
        } else {
            WINDOW_SIZE
        };

        let mut min_price = Price::MAX;
        let mut max_price: Price = 0;
        let mut sum: u128 = 0;

        for i in 0..window_end {
            let price = self.prices[i];
            if price < min_price {
                min_price = price;
            }
            if price > max_price {
                max_price = price;
            }
            sum += price as u128;
        }

        if min_price == 0 {
            return 0;
        }

        let mean = sum / (window_end as u128);
        if mean == 0 {
            return 0;
        }

        // Vol = (max - min) / mean * 10000 (bps)
        let range = (max_price as u128) - (min_price as u128);
        let vol_bps = (range * 10_000) / mean;

        // Cap at reasonable max (10000 bps = 100%)
        if vol_bps > 10_000 {
            10_000
        } else {
            vol_bps as u32
        }
    }

    /// Calculate returns-based volatility (standard deviation of log returns)
    /// More accurate but requires more data
    pub fn volatility_bps_accurate(&self) -> u32 {
        if !self.ready || self.count < 3 {
            return self.volatility_bps();
        }

        let window_end = if self.count < WINDOW_SIZE {
            self.count
        } else {
            WINDOW_SIZE
        };

        // Calculate log returns
        let mut returns: [i64; WINDOW_SIZE - 1] = [0; WINDOW_SIZE - 1];
        let mut n = 0;

        for i in 1..window_end {
            let prev = self.prices[i - 1] as u128;
            let curr = self.prices[i] as u128;

            if prev == 0 || curr == 0 {
                continue;
            }

            // Log return: ln(curr / prev) * 10000 (basis points)
            let ratio = (curr * 10_000) / prev;
            let log_return: i64 = if ratio > 10_000 {
                // Simple approximation: ratio - 10000 for small changes
                (ratio - 10_000) as i64
            } else {
                // ratio <= 10000, so (10000 - ratio) is positive
                -((10_000 - ratio) as i64)
            };

            returns[n] = log_return;
            n += 1;
        }

        if n < 2 {
            return self.volatility_bps();
        }

        // Calculate mean
        let sum: i64 = returns.iter().take(n).sum();
        let mean = sum / (n as i64);

        // Calculate variance
        let mut variance_sum: i128 = 0;
        for i in 0..n {
            let diff = (returns[i] - mean) as i128;
            variance_sum += diff * diff;
        }

        let variance = variance_sum / ((n - 1) as i128);
        let std_dev = (variance as f64).sqrt() as u64;

        // Annualize (square root of 24 for hourly-ish data)
        // Return as bps
        let vol_bps = (std_dev * 100) as u32; // Simplified scaling

        if vol_bps > 10_000 {
            10_000
        } else {
            vol_bps
        }
    }

    /// Reset the estimator
    pub fn reset(&mut self) {
        self.count = 0;
        self.head = 0;
        self.last_price = 0;
        self.ready = false;
    }
}

impl Default for VolatilityEstimator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_volatility_zero_prices() {
        let mut est = VolatilityEstimator::new();
        assert_eq!(est.volatility_bps(), 0);
    }

    #[test]
    fn test_volatility_single_price() {
        let mut est = VolatilityEstimator::new();
        est.update(100_000);
        assert_eq!(est.volatility_bps(), 0);
    }

    #[test]
    fn test_volatility_stable_market() {
        let mut est = VolatilityEstimator::new();
        // Stable prices around 100
        for _ in 0..10 {
            est.update(100_000);
        }
        // Should be very low volatility
        let vol = est.volatility_bps();
        assert!(vol < 100);
    }

    #[test]
    fn test_volatility_volatile_market() {
        let mut est = VolatilityEstimator::new();
        // Volatile: up and down
        est.update(100_000);
        est.update(105_000);
        est.update(100_000);
        est.update(95_000);
        est.update(100_000);
        
        let vol = est.volatility_bps();
        // Should be significant
        assert!(vol > 500);
    }

    #[test]
    fn test_current_price() {
        let mut est = VolatilityEstimator::new();
        assert_eq!(est.current_price(), 0);
        
        est.update(100_000);
        assert_eq!(est.current_price(), 100_000);
        
        est.update(105_000);
        assert_eq!(est.current_price(), 105_000);
    }
}
