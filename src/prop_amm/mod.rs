//! Prop-AMM Pricing Engine
//!
//! Dynamic pricing that adapts to market conditions.
//! Uses the existing vAMM infrastructure but with smart parameter calculation.

#![no_std]

extern crate alloc;

/// Volatility estimator using simple rolling window
pub mod volatility;

/// Dynamic spread calculator
pub mod spread;

/// Types for the pricing engine
pub mod types;

/// Integration with on-chain vAMM
pub mod integration;

/// Off-chain keeper (requires std)
#[cfg(feature = "std")]
pub mod keeper;

pub use volatility::VolatilityEstimator;
pub use spread::SpreadCalculator;
pub use types::*;
pub use integration::{PropAmmUpdateParams, PROP_AMM_UPDATE_TAG};
