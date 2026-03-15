//! Dynamic Spread Calculator
//!
//! Calculates optimal spreads based on market conditions and inventory.

use super::types::*;

/// Spread calculator for prop-AMM
#[derive(Clone, Debug)]
pub struct SpreadCalculator {
    /// Configuration
    config: Config,
}

impl SpreadCalculator {
    /// Create a new calculator with config
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    /// Create with default config
    pub fn default() -> Self {
        Self {
            config: Config::default(),
        }
    }

    /// Calculate pricing parameters based on market conditions
    pub fn calculate(&self, market: &MarketConditions, inventory_pct: u8) -> PricingParams {
        // Start with base calculation
        let base_spread = self.calculate_base_spread(market);
        
        // Add inventory risk premium
        let inventory_premium = self.calculate_inventory_premium(inventory_pct);
        
        // Combine
        let total_spread = base_spread.saturating_add(inventory_premium);
        
        // Cap at max
        let final_spread = total_spread.min(self.config.max_spread_bps);

        // Calculate impact k based on market liquidity
        let impact_k = self.calculate_impact_k(market);
        
        // Calculate max fill based on volume
        let max_fill = self.calculate_max_fill(market);
        
        // Calculate max inventory
        let max_inventory = self.calculate_max_inventory(market);

        PricingParams {
            base_spread_bps: final_spread,
            fee_bps: self.calculate_fee(market),
            impact_k_bps: impact_k,
            max_fill,
            max_inventory,
        }
    }

    /// Calculate base spread from market conditions
    fn calculate_base_spread(&self, market: &MarketConditions) -> u32 {
        // If no volume data, use wide spread
        if market.volume_24h == 0 {
            return self.config.wide_spread_bps;
        }

        // Volume-based spread
        let volume_spread = if market.volume_24h >= self.config.liquid_threshold {
            self.config.tight_spread_bps
        } else {
            // Linear interpolation between tight and wide
            let ratio = market.volume_24h * 100 / self.config.liquid_threshold;
            let range = self.config.wide_spread_bps - self.config.tight_spread_bps;
            self.config.tight_spread_bps + ((range * ratio as u32) / 100)
        };

        // Volatility adjustment
        let vol_spread = if market.volatility_bps >= self.config.vol_high_bps {
            // High volatility: use max spread
            self.config.max_spread_bps
        } else if market.volatility_bps <= self.config.vol_low_bps {
            // Low volatility: no extra spread
            0
        } else {
            // Interpolate
            let ratio = (market.volatility_bps - self.config.vol_low_bps) * 100 
                / (self.config.vol_high_bps - self.config.vol_low_bps);
            let extra = ((self.config.max_spread_bps - self.config.tight_spread_bps) * ratio as u32) / 100;
            extra
        };

        volume_spread.saturating_add(vol_spread)
    }

    /// Calculate inventory risk premium
    fn calculate_inventory_premium(&self, inventory_pct: u8) -> u32 {
        if inventory_pct <= self.config.max_inventory_pct {
            return 0;
        }

        // How far over the threshold
        let overage = inventory_pct as u32 - self.config.max_inventory_pct as u32;
        let max_overage = 100 - self.config.max_inventory_pct as u32;
        
        // Scale from 0 to inventory_risk_bps
        let premium = (overage as u32 * self.config.inventory_risk_bps) / max_overage;
        
        premium.min(self.config.inventory_risk_bps)
    }

    /// Calculate impact coefficient (higher for thin markets)
    fn calculate_impact_k(&self, market: &MarketConditions) -> u32 {
        if market.volume_24h == 0 {
            return 200; // High impact for no volume
        }

        if market.volume_24h >= self.config.liquid_threshold {
            return 50; // Low impact for liquid markets
        }

        // Interpolate
        let ratio = market.volume_24h * 100 / self.config.liquid_threshold;
        50 + ((150 * (100 - ratio as u32)) / 100)
    }

    /// Calculate max fill size
    fn calculate_max_fill(&self, market: &MarketConditions) -> Quantity {
        if market.volume_24h == 0 {
            return 1_000; // Small fills for thin markets
        }

        // Max fill = 1% of 24h volume (in base units)
        // Approximate: volume / oracle_price * 0.01
        if market.oracle_price == 0 {
            return 10_000;
        }

        let base_volume = market.volume_24h / market.oracle_price as u128;
        let max_fill = (base_volume / 100) as Quantity;

        // Cap at reasonable max
        if max_fill > 1_000_000 {
            1_000_000
        } else if max_fill < 1_000 {
            1_000
        } else {
            max_fill
        }
    }

    /// Calculate max inventory
    fn calculate_max_inventory(&self, market: &MarketConditions) -> Quantity {
        if market.open_interest == 0 {
            return 10_000;
        }

        // Max inventory = 10% of open interest
        let max_inv = (market.open_interest.unsigned_abs() / 10) as Quantity;
        
        // Cap
        if max_inv > 100_000 {
            100_000
        } else if max_inv < 1_000 {
            1_000
        } else {
            max_inv
        }
    }

    /// Calculate trading fee
    fn calculate_fee(&self, market: &MarketConditions) -> u32 {
        if market.volume_24h >= self.config.liquid_threshold {
            2 // 0.02% for liquid markets
        } else if market.volume_24h >= self.config.liquid_threshold / 10 {
            5 // 0.05% for medium
        } else {
            10 // 0.1% for thin
        }
    }

    /// Update configuration
    pub fn set_config(&mut self, config: Config) {
        self.config = config;
    }

    /// Get current config
    pub fn config(&self) -> &Config {
        &self.config
    }
}

impl Default for SpreadCalculator {
    fn default() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn liquid_market() -> MarketConditions {
        MarketConditions {
            oracle_price: 100_000_000, // $100
            volume_24h: 10_000_000_000_000, // $10M
            open_interest: 1_000_000,
            trade_count_24h: 10_000,
            volatility_bps: 50,
        }
    }

    fn thin_market() -> MarketConditions {
        MarketConditions {
            oracle_price: 100_000_000,
            volume_24h: 100_000_000, // $100K
            open_interest: 1_000,
            trade_count_24h: 10,
            volatility_bps: 500,
        }
    }

    #[test]
    fn test_liquid_market_tight_spread() {
        let calc = SpreadCalculator::default();
        let params = calc.calculate(&liquid_market(), 0);
        
        assert!(params.base_spread_bps <= 10);
    }

    #[test]
    fn test_thin_market_wide_spread() {
        let calc = SpreadCalculator::default();
        let params = calc.calculate(&thin_market(), 0);
        
        assert!(params.base_spread_bps >= 50);
    }

    #[test]
    fn test_inventory_risk() {
        let calc = SpreadCalculator::default();
        
        // At 0% inventory, no premium
        let params0 = calc.calculate(&liquid_market(), 0);
        
        // At 90% inventory, premium applies
        let params90 = calc.calculate(&liquid_market(), 90);
        
        assert!(params90.base_spread_bps > params0.base_spread_bps);
    }

    #[test]
    fn test_impact_k_liquid() {
        let calc = SpreadCalculator::default();
        let params = calc.calculate(&liquid_market(), 0);
        
        assert!(params.impact_k_bps <= 100);
    }

    #[test]
    fn test_impact_k_thin() {
        let calc = SpreadCalculator::default();
        let params = calc.calculate(&thin_market(), 0);
        
        assert!(params.impact_k_bps >= 100);
    }
}
