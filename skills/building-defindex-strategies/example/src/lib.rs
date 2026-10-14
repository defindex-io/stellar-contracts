#![no_std]
//! Reference DeFindex strategy: the vault-facing contract every strategy needs, with the
//! protocol-specific part isolated in `upstream.rs`.
//!
//! What the vault relies on, and where this file honours it:
//! - `deposit` pulls exactly `transfer(from -> self, amount)` once and returns `from`'s
//!   balance AFTER the deposit, in underlying units.
//! - `withdraw` delivers exactly `amount` to `to` and returns `from`'s REMAINING balance.
//!   It is never allowlist-gated or paused, so a vault can always exit.
//! - `balance` is a read that never panics and needs no auth.
//! - `harvest` is not called by the vault. A keeper calls it.

use defindex_strategy_core::{event, DeFindexStrategyTrait, StrategyError};
use soroban_sdk::{
    contract, contractevent, contractimpl, token::TokenClient, Address, Bytes, BytesN, Env,
    Executable, IntoVal, String, Val, Vec,
};

mod shares;
mod storage;
mod upstream;

#[cfg(test)]
mod test;
#[cfg(all(test, feature = "vault-it"))]
mod test_vault;

use storage::Config;

const STRATEGY_NAME: &str = "ExampleStrategy";

fn name(e: &Env) -> String {
    String::from_str(e, STRATEGY_NAME)
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DepositorSet {
    #[topic]
    pub strategy_name: String,
    pub depositor: Address,
    pub allowed: bool,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AllowedHashSet {
    #[topic]
    pub strategy_name: String,
    pub hash: BytesN<32>,
    pub allowed: bool,
}

fn check_positive(amount: i128) -> Result<(), StrategyError> {
    if amount <= 0 {
        return Err(StrategyError::OnlyPositiveAmountAllowed);
    }
    Ok(())
}

/// Deposits fail closed. Without this, any account can deposit directly, hold its own
/// position, and skip the vault's fee layer. Allowlisting the vault WASM hash lets every
/// vault built from that code deposit without per-address curation.
fn is_authorized_depositor(e: &Env, from: &Address) -> bool {
    if storage::is_depositor(e, from) {
        return true;
    }
    match from.executable() {
        Some(Executable::Wasm(hash)) => storage::is_allowed_hash(e, &hash),
        _ => false,
    }
}

/// Keepers pass a 16-byte big-endian i128 `min_out` for the reward swap. Reject anything
/// else with an error instead of panicking or silently defaulting to "no floor".
fn decode_min_out(data: &Option<Bytes>) -> Result<Option<i128>, StrategyError> {
    match data {
        None => Ok(None),
        Some(bytes) if bytes.is_empty() => Ok(None),
        Some(bytes) if bytes.len() == 16 => {
            let mut buf = [0u8; 16];
            bytes.copy_into_slice(&mut buf);
            Ok(Some(i128::from_be_bytes(buf)))
        }
        Some(_) => Err(StrategyError::InvalidArgument),
    }
}

#[contract]
pub struct ExampleStrategy;

#[contractimpl]
impl DeFindexStrategyTrait for ExampleStrategy {
    /// `init_args = [admin: Address, keeper: Address]`. Panicking is acceptable only here:
    /// a bad deploy should fail loudly, before any vault points at it.
    fn __constructor(e: Env, asset: Address, init_args: Vec<Val>) {
        let admin: Address = init_args.get(0).expect("missing admin").into_val(&e);
        let keeper: Address = init_args.get(1).expect("missing keeper").into_val(&e);
        storage::set_config(&e, &Config { asset, admin, keeper });
        storage::set_total_shares(&e, 0);
        storage::extend_instance_ttl(&e);
    }

    /// The vault checks this equals the asset it lists the strategy under, once, at vault
    /// construction. It must never change.
    fn asset(e: Env) -> Result<Address, StrategyError> {
        storage::extend_instance_ttl(&e);
        Ok(storage::get_config(&e).asset)
    }

    /// Returns `from`'s balance AFTER the deposit, in underlying units. The vault records
    /// `returned - previous_balance - amount` as gain or loss, so returning `amount` (or
    /// shares) corrupts its fee accounting.
    fn deposit(e: Env, amount: i128, from: Address) -> Result<i128, StrategyError> {
        storage::extend_instance_ttl(&e);
        check_positive(amount)?;
        from.require_auth();
        if !is_authorized_depositor(&e, &from) {
            return Err(StrategyError::NotAuthorized);
        }

        let cfg = storage::get_config(&e);
        // Price shares on NAV BEFORE the tokens arrive.
        let nav_before = upstream::nav(&e, &cfg);

        // The vault pre-authorizes exactly this call: transfer(vault, strategy, amount).
        // Pulling a different amount, or sending it anywhere else, fails its auth.
        TokenClient::new(&e, &cfg.asset).transfer(&from, &e.current_contract_address(), &amount);
        upstream::supply(&e, &cfg, amount)?;

        // Mint on the value actually added, not on `amount`: protocols round and charge.
        let nav_after = upstream::nav(&e, &cfg);
        let delta = nav_after
            .checked_sub(nav_before)
            .ok_or(StrategyError::UnderflowOverflow)?;
        let total = storage::get_total_shares(&e);
        let minted = if total == 0 {
            // First depositor (or first after a full exit) gets value left behind, 1:1.
            delta
                .checked_mul(shares::VIRTUAL_SHARES)
                .ok_or(StrategyError::UnderflowOverflow)?
        } else {
            shares::to_shares_down(&e, delta, total, nav_before)
        };
        if minted <= 0 {
            return Err(StrategyError::InvalidSharesMinted);
        }

        let new_shares = storage::peek_shares(&e, &from)
            .checked_add(minted)
            .ok_or(StrategyError::UnderflowOverflow)?;
        let new_total = total
            .checked_add(minted)
            .ok_or(StrategyError::UnderflowOverflow)?;
        storage::set_shares(&e, &from, new_shares);
        storage::set_total_shares(&e, new_total);

        event::emit_deposit(&e, name(&e), amount, from);
        Ok(shares::to_assets_down(&e, new_shares, new_total, nav_after))
    }

    /// Not called by the vault. Keeper-only: an open harvest lets anyone trigger reward
    /// swaps at a price they chose. Below-threshold or nothing-to-claim is a no-op, not an
    /// error, so a keeper loop never stalls on it.
    fn harvest(e: Env, from: Address, data: Option<Bytes>) -> Result<(), StrategyError> {
        storage::extend_instance_ttl(&e);
        let cfg = storage::get_config(&e);
        cfg.keeper.require_auth();
        if from != cfg.keeper {
            return Err(StrategyError::NotAuthorized);
        }
        let _min_out = decode_min_out(&data)?;

        // Real strategy: claim rewards; if zero, return Ok(()). Swap them to `asset` with
        // `min_out` (require it to be > 0 whenever a swap runs), then `upstream::supply`
        // the proceeds WITHOUT minting shares, so every holder's balance rises.
        // Consider releasing that profit linearly (vesting) so a deposit just before
        // harvest and a withdraw just after cannot capture it.
        let harvested: i128 = 0;

        let nav = upstream::nav(&e, &cfg);
        let pps = shares::price_per_share(&e, nav, storage::get_total_shares(&e));
        event::emit_harvest(&e, name(&e), harvested, from, pps);
        Ok(())
    }

    /// `from`'s balance in underlying units. MUST NOT PANIC and needs no auth: the vault
    /// calls this for every strategy, paused or not, on every deposit and withdraw. One
    /// failing strategy blocks the whole vault, and strategies cannot be removed.
    fn balance(e: Env, from: Address) -> Result<i128, StrategyError> {
        // Pure read: no TTL writes, no events.
        let held = storage::peek_shares(&e, &from);
        if held == 0 {
            return Ok(0);
        }
        let cfg = storage::get_config(&e);
        let nav = upstream::nav(&e, &cfg);
        Ok(shares::to_assets_down(&e, held, storage::get_total_shares(&e), nav))
    }

    /// Sends exactly `amount` to `to` (the end user on a vault withdraw, the vault on a
    /// rebalance) and returns `from`'s REMAINING balance. Never allowlist-gated or paused.
    fn withdraw(e: Env, amount: i128, from: Address, to: Address) -> Result<i128, StrategyError> {
        storage::extend_instance_ttl(&e);
        check_positive(amount)?;
        from.require_auth();

        let cfg = storage::get_config(&e);
        let nav = upstream::nav(&e, &cfg);
        let total = storage::get_total_shares(&e);
        let held = storage::peek_shares(&e, &from);
        if amount > shares::to_assets_down(&e, held, total, nav) {
            return Err(StrategyError::InsufficientBalance);
        }

        // Ceil burn, clamped so a full exit burns exactly what is held.
        let burn = shares::to_shares_up(&e, amount, total, nav).min(held);
        let remaining = held - burn;
        let new_total = total - burn;
        // Effects before the external call.
        storage::set_shares(&e, &from, remaining);
        storage::set_total_shares(&e, new_total);

        upstream::withdraw_to(&e, &cfg, amount, &to)?;

        event::emit_withdraw(&e, name(&e), amount, from);
        // Re-measure instead of assuming NAV fell by exactly `amount`: exit fees and
        // protocol rounding make the real drop larger.
        let nav_after = upstream::nav(&e, &cfg);
        Ok(shares::to_assets_down(&e, remaining, new_total, nav_after))
    }
}

/// Admin surface, outside the trait. The vault never calls these.
#[contractimpl]
impl ExampleStrategy {
    pub fn set_depositor(e: Env, depositor: Address, allowed: bool) {
        storage::extend_instance_ttl(&e);
        storage::get_config(&e).admin.require_auth();
        storage::set_depositor(&e, &depositor, allowed);
        DepositorSet { strategy_name: name(&e), depositor, allowed }.publish(&e);
    }

    /// Allowlist every contract running this WASM, e.g. the DeFindex vault WASM hash.
    /// A vault that upgrades its WASM changes hash and must be re-allowlisted.
    pub fn set_allowed_hash(e: Env, hash: BytesN<32>, allowed: bool) {
        storage::extend_instance_ttl(&e);
        storage::get_config(&e).admin.require_auth();
        storage::set_allowed_hash(&e, &hash, allowed);
        AllowedHashSet { strategy_name: name(&e), hash, allowed }.publish(&e);
    }
}
