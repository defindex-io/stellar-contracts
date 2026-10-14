//! The only module you replace. Everything protocol-specific lives behind these three
//! functions; lib.rs keeps the vault-facing contract.
//!
//! This example's "protocol" is the strategy holding the token itself, so it compiles and
//! tests with no external contracts. A real strategy supplies to a lending pool, an AMM,
//! etc. Keep the same contract for each function:
//!
//! - `nav` is READ-ONLY and MUST NOT PANIC. `balance()` calls it, and the vault calls
//!   `balance()` on every strategy on every deposit and withdraw. Use `try_` client calls
//!   and fall back to a conservative value (last known good, or 0 for the failing part).
//!   Never use a spot price someone can move inside one transaction.
//! - `supply` moves `amount` the strategy already holds into the protocol.
//! - `withdraw_to` delivers EXACTLY `amount` of the underlying to `to`, or returns an error.
//!   Never deliver less and report success: the vault does not check what arrived.

use crate::storage::Config;
use defindex_strategy_core::StrategyError;
use soroban_sdk::{token::TokenClient, Address, Env};

/// Total underlying the strategy controls, across all depositors.
pub fn nav(e: &Env, cfg: &Config) -> i128 {
    // A SAC `balance` read cannot fail. For a custom token or a protocol position, use
    // `try_balance` / `try_get_position` and degrade instead of propagating the error.
    TokenClient::new(e, &cfg.asset).balance(&e.current_contract_address())
}

pub fn supply(_e: &Env, _cfg: &Config, _amount: i128) -> Result<(), StrategyError> {
    // Real strategy: call the protocol here. If the protocol pulls tokens from the
    // strategy, pre-authorize exactly that transfer with `e.authorize_as_current_contract`.
    Ok(())
}

pub fn withdraw_to(e: &Env, cfg: &Config, amount: i128, to: &Address) -> Result<(), StrategyError> {
    // Real strategy: redeem from the protocol, ideally straight to `to`. If the protocol
    // can return less than asked (rounding, fees, illiquidity), check and return
    // `InsufficientBalance` rather than under-paying.
    TokenClient::new(e, &cfg.asset).transfer(&e.current_contract_address(), to, &amount);
    Ok(())
}
