//! Dynamic Prop-AMM Integration
//!
//! Adds dynamic parameter updates to the vAMM for real-time spread adjustment.
//! This enables the prop-AMM pricing engine to adapt to market conditions.

use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    program_error::ProgramError,
    pubkey::Pubkey,
};

use crate::{
    MatcherCtx, MatcherCall, MatcherReturn,
    CTX_VAMM_OFFSET, MATCHER_RETURN_LEN,
    MATCHER_ABI_VERSION, FLAG_VALID,
};

/// Instruction tag for dynamic update
pub const PROP_AMM_UPDATE_TAG: u8 = 3;

/// Update params instruction layout:
/// Offset  Field                   Type    Size
/// 0       tag                     u8      1     Always 3
/// 1-5     base_spread_bps         u32     4
/// 5-9     trading_fee_bps         u32     4
/// 9-13    impact_k_bps            u32     4
/// 13-21   max_fill_abs            u128    8
/// 21-29   max_inventory_abs       u128    8
/// Total: 29 bytes
pub const PROP_AMM_UPDATE_LEN: usize = 29;

/// Parse update params from instruction data
#[derive(Clone, Copy, Debug)]
pub struct PropAmmUpdateParams {
    pub base_spread_bps: u32,
    pub trading_fee_bps: u32,
    pub impact_k_bps: u32,
    pub max_fill_abs: u128,
    pub max_inventory_abs: u128,
}

impl PropAmmUpdateParams {
    pub fn parse(data: &[u8]) -> Result<Self, ProgramError> {
        if data.len() < PROP_AMM_UPDATE_LEN {
            return Err(ProgramError::InvalidInstructionData);
        }
        if data[0] != PROP_AMM_UPDATE_TAG {
            return Err(ProgramError::InvalidInstructionData);
        }

        Ok(Self {
            base_spread_bps: u32::from_le_bytes(data[1..5].try_into().unwrap()),
            trading_fee_bps: u32::from_le_bytes(data[5..9].try_into().unwrap()),
            impact_k_bps: u32::from_le_bytes(data[9..13].try_into().unwrap()),
            max_fill_abs: u128::from_le_bytes(data[13..21].try_into().unwrap()),
            max_inventory_abs: u128::from_le_bytes(data[21..29].try_into().unwrap()),
        })
    }

    /// Encode to instruction data
    pub fn encode(&self) -> [u8; PROP_AMM_UPDATE_LEN] {
        let mut data = [0u8; PROP_AMM_UPDATE_LEN];
        data[0] = PROP_AMM_UPDATE_TAG;
        data[1..5].copy_from_slice(&self.base_spread_bps.to_le_bytes());
        data[5..9].copy_from_slice(&self.trading_fee_bps.to_le_bytes());
        data[9..13].copy_from_slice(&self.impact_k_bps.to_le_bytes());
        data[13..21].copy_from_slice(&self.max_fill_abs.to_le_bytes());
        data[21..29].copy_from_slice(&self.max_inventory_abs.to_le_bytes());
        data
    }
}

/// Process dynamic param update instruction (Tag 3)
///
/// Allows off-chain keeper to update pricing parameters in real-time.
/// This enables the prop-AMM to adapt spreads based on market conditions.
///
/// Accounts:
/// 0. `[signer]` LP PDA (must match stored PDA - authorization)
/// 1. `[writable]` Matcher context account
pub fn process_prop_amm_update(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    let account_iter = &mut accounts.iter();
    let lp_pda = next_account_info(account_iter)?;
    let ctx_account = next_account_info(account_iter)?;

    // Verify ownership
    if ctx_account.owner != program_id {
        return Err(ProgramError::IncorrectProgramId);
    }

    // Verify minimum size
    if ctx_account.data_len() < 320 {
        return Err(ProgramError::AccountDataTooSmall);
    }

    // Require LP PDA signature (authorization)
    if !lp_pda.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }

    // Parse update params
    let params = PropAmmUpdateParams::parse(instruction_data)?;

    // Read current context
    let mut ctx = {
        let data = ctx_account.try_borrow_data()?;
        MatcherCtx::read_from(&data[CTX_VAMM_OFFSET..])?
    };

    // Verify LP PDA matches (authorization)
    if lp_pda.key.to_bytes() != ctx.lp_pda {
        return Err(ProgramError::InvalidAccountData);
    }

    // Validate new params
    let total_fixed = params.base_spread_bps.saturating_add(params.trading_fee_bps);
    if total_fixed > ctx.max_total_bps {
        return Err(ProgramError::InvalidAccountData);
    }

    // Update params
    ctx.base_spread_bps = params.base_spread_bps;
    ctx.trading_fee_bps = params.trading_fee_bps;
    ctx.impact_k_bps = params.impact_k_bps;
    ctx.max_fill_abs = params.max_fill_abs;
    ctx.max_inventory_abs = params.max_inventory_abs;

    // Write updated context
    let mut data = ctx_account.try_borrow_mut_data()?;
    ctx.write_to(&mut data[CTX_VAMM_OFFSET..])?;

    // Emit success event (return current state)
    let ret = MatcherReturn {
        abi_version: MATCHER_ABI_VERSION,
        flags: FLAG_VALID,
        exec_price_e6: ctx.base_spread_bps as u64, // Reuse field for confirmation
        exec_size: ctx.trading_fee_bps as i128,
        req_id: 0,
        lp_account_id: 0,
        oracle_price_e6: ctx.impact_k_bps as u64,
        // Admin params-update confirmation, not a trade return — the engine never
        // validates this one, so there is no asset to echo.
        asset_index: 0,
    };
    ret.write_to(&mut data[..MATCHER_RETURN_LEN])?;

    Ok(())
}

/// Read current pricing params from context (for keeper polling)
#[derive(Clone, Copy, Debug)]
pub struct CurrentParams {
    pub base_spread_bps: u32,
    pub trading_fee_bps: u32,
    pub impact_k_bps: u32,
    pub max_fill_abs: u128,
    pub max_inventory_abs: u128,
    pub inventory_base: i128,
    pub last_oracle_price_e6: u64,
    pub last_exec_price_e6: u64,
}

impl CurrentParams {
    pub fn from_ctx(ctx: &MatcherCtx) -> Self {
        Self {
            base_spread_bps: ctx.base_spread_bps,
            trading_fee_bps: ctx.trading_fee_bps,
            impact_k_bps: ctx.impact_k_bps,
            max_fill_abs: ctx.max_fill_abs,
            max_inventory_abs: ctx.max_inventory_abs,
            inventory_base: ctx.inventory_base,
            last_oracle_price_e6: ctx.last_oracle_price_e6,
            last_exec_price_e6: ctx.last_exec_price_e6,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_params_encode_decode() {
        let params = PropAmmUpdateParams {
            base_spread_bps: 10,
            trading_fee_bps: 5,
            impact_k_bps: 100,
            max_fill_abs: 1_000_000,
            max_inventory_abs: 100_000,
        };

        let encoded = params.encode();
        let decoded = PropAmmUpdateParams::parse(&encoded).unwrap();

        assert_eq!(params.base_spread_bps, decoded.base_spread_bps);
        assert_eq!(params.trading_fee_bps, decoded.trading_fee_bps);
        assert_eq!(params.impact_k_bps, decoded.impact_k_bps);
        assert_eq!(params.max_fill_abs, decoded.max_fill_abs);
        assert_eq!(params.max_inventory_abs, decoded.max_inventory_abs);
    }

    #[test]
    fn test_update_params_wrong_tag() {
        let mut data = [0u8; PROP_AMM_UPDATE_LEN];
        data[0] = 99; // Wrong tag
        assert!(PropAmmUpdateParams::parse(&data).is_err());
    }

    #[test]
    fn test_update_params_too_short() {
        let data = [0u8; 10];
        assert!(PropAmmUpdateParams::parse(&data).is_err());
    }
}
