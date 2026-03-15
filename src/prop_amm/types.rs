//! Types for the prop-AMM pricing engine

use core::ops::{Add, Sub, Mul, Div};

/// Price in 1e6 scale (same as Percolator oracle)
pub type Price = u64;

/// Quantity in base units
pub type Quantity = i128;

/// Spread in basis points (100 bps = 1%)
pub type SpreadBps = u32;

/// Volume in notional (1e6 scale)
pub type Notional = u128;

/// Result type for pricing calculations
pub type Result<T> = core::result::Result<T, Error>;

/// Errors in pricing calculations
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Error {
    /// Oracle price is zero
    OracleZero = 0,
    /// Overflow in calculation
    Overflow = 1,
    /// Underflow in calculation
    Underflow = 2,
    /// Invalid parameter
    InvalidParam = 3,
    /// Not enough data for calculation
    InsufficientData = 4,
}

impl Error {
    pub fn as_str(&self) -> &'static str {
        match self {
            Error::OracleZero => "oracle_zero",
            Error::Overflow => "overflow",
            Error::Underflow => "underflow",
            Error::InvalidParam => "invalid_param",
            Error::InsufficientData => "insufficient_data",
        }
    }
}

/// Market conditions for pricing
#[derive(Clone, Copy, Debug, Default)]
pub struct MarketConditions {
    /// Current oracle price
    pub oracle_price: Price,
    /// 24h volume in notional
    pub volume_24h: Notional,
    /// Open interest in base units
    pub open_interest: Quantity,
    /// Number of trades in 24h
    pub trade_count_24h: u64,
    /// Volatility estimate (0-10000 bps)
    pub volatility_bps: u32,
}

/// Configuration for the prop-AMM
#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// Minimum base spread (bps)
    pub min_spread_bps: u32,
    /// Maximum base spread (bps)
    pub max_spread_bps: u32,
    /// Target spread for highly liquid markets (bps)
    pub tight_spread_bps: u32,
    /// Spread for thin markets (bps)
    pub wide_spread_bps: u32,
    /// Volume threshold to consider market liquid (notional 1e6)
    pub liquid_threshold: Notional,
    /// Volatility threshold for wide spread (bps)
    pub vol_high_bps: u32,
    /// Volatility threshold for tight spread (bps)
    pub vol_low_bps: u32,
    /// Inventory risk factor (bps per 100% of max inventory)
    pub inventory_risk_bps: u32,
    /// Maximum inventory as % of pool (0-100)
    pub max_inventory_pct: u8,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            min_spread_bps: 2,        // 0.02% - very tight
            max_spread_bps: 500,      // 5% - max spread cap
            tight_spread_bps: 3,      // 0.03% - BTC/SOL level
            wide_spread_bps: 100,    // 1% - for thin markets
            liquid_threshold: 1_000_000_000_000, // $1M notional
            vol_high_bps: 500,       // 5% daily volatility = wide
            vol_low_bps: 100,        // 1% daily volatility = tight
            inventory_risk_bps: 50,  // 0.5% extra spread per 100% inventory
            max_inventory_pct: 80,   // Start widening at 80% of max
        }
    }
}

/// Computed pricing parameters
#[derive(Clone, Copy, Debug, Default)]
pub struct PricingParams {
    /// Base spread to use (bps)
    pub base_spread_bps: u32,
    /// Trading fee (bps)
    pub fee_bps: u32,
    /// Impact coefficient for vAMM (bps)
    pub impact_k_bps: u32,
    /// Max fill size
    pub max_fill: Quantity,
    /// Max inventory
    pub max_inventory: Quantity,
}

impl PricingParams {
    /// Create with all values
    pub fn new(
        base_spread_bps: u32,
        fee_bps: u32,
        impact_k_bps: u32,
        max_fill: Quantity,
        max_inventory: Quantity,
    ) -> Self {
        Self {
            base_spread_bps,
            fee_bps,
            impact_k_bps,
            max_fill,
            max_inventory,
        }
    }

    /// Default params for liquid markets
    pub fn liquid() -> Self {
        Self {
            base_spread_bps: 3,
            fee_bps: 2,
            impact_k_bps: 50,
            max_fill: 1_000_000, // 1M base units
            max_inventory: 100_000,
        }
    }

    /// Default params for thin markets
    pub fn thin() -> Self {
        Self {
            base_spread_bps: 100,
            fee_bps: 5,
            impact_k_bps: 200,
            max_fill: 10_000, // Very small fills
            max_inventory: 1_000,
        }
    }
}
