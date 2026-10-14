//! Virtual-offset share accounting (OpenZeppelin ERC-4626 v5 style).
//!
//! `VIRTUAL_SHARES` sits on the shares side of every conversion and a virtual `+1` on the
//! assets side. That makes the first-depositor inflation attack cost roughly
//! `VIRTUAL_SHARES` times the victim's deposit, without burning real shares. A fixed
//! "burn 1000 units on first deposit" does not scale across token decimals; this does.
//!
//! Rounding always favours the strategy (the remaining holders):
//! floor on mint, ceil on burn, floor on the balance reported to the vault.
//!
//! `SorobanFixedPoint` (the `env` variant) retries in I256 when `x * y` overflows i128.
//! The plain `FixedPoint` trait returns `None` instead, and because shares are roughly
//! `nav * VIRTUAL_SHARES`, `shares * nav` overflows at about 1.3e16 base units of NAV:
//! ~1.3B tokens at 7 decimals, but ~0.013 tokens at 18 decimals. Use the env variant.

use soroban_fixed_point_math::SorobanFixedPoint;
use soroban_sdk::Env;

pub const VIRTUAL_SHARES: i128 = 1_000_000;
/// Scale for the harvest event's `price_per_share`. Indexers divide by 1e12.
pub const SCALAR_12: i128 = 1_000_000_000_000;

// Neither denominator can be zero: both carry a positive virtual term, and total shares
// and NAV are never negative. checked_add only fails at i128::MAX, unreachable for real
// token supplies.
fn shares_den(total_shares: i128) -> i128 {
    total_shares.checked_add(VIRTUAL_SHARES).unwrap()
}

fn nav_den(nav: i128) -> i128 {
    nav.checked_add(1).unwrap()
}

/// Shares to mint for `delta` underlying added on top of `nav_before`. Floor.
pub fn to_shares_down(e: &Env, delta: i128, total_shares: i128, nav_before: i128) -> i128 {
    delta.fixed_mul_floor(e, &shares_den(total_shares), &nav_den(nav_before))
}

/// Shares to burn so the withdrawer receives `amount`. Ceil, so we never under-burn.
pub fn to_shares_up(e: &Env, amount: i128, total_shares: i128, nav: i128) -> i128 {
    amount.fixed_mul_ceil(e, &shares_den(total_shares), &nav_den(nav))
}

/// Underlying value of `shares`. Floor.
pub fn to_assets_down(e: &Env, shares: i128, total_shares: i128, nav: i128) -> i128 {
    shares.fixed_mul_floor(e, &nav_den(nav), &shares_den(total_shares))
}

/// Price per share scaled by 1e12, using the same virtual denominator as the share math.
pub fn price_per_share(e: &Env, nav: i128, total_shares: i128) -> i128 {
    nav.fixed_mul_floor(e, &SCALAR_12, &shares_den(total_shares))
}
