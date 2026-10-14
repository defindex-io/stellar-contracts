//! Unit tests: the invariants a vault depends on. `mock_all_auths` is used for setup only;
//! `harvest_rejects_non_keeper` shows how to test auth for real.

extern crate std;

use crate::{ExampleStrategy, ExampleStrategyClient};
use defindex_strategy_core::StrategyError;
use soroban_sdk::{
    testutils::{Address as _, MockAuth, MockAuthInvoke},
    token::{StellarAssetClient, TokenClient},
    vec, Address, Bytes, Env, IntoVal, Val, Vec,
};

struct Setup<'a> {
    e: Env,
    strategy: ExampleStrategyClient<'a>,
    token: TokenClient<'a>,
    minter: StellarAssetClient<'a>,
    keeper: Address,
    vault: Address,
}

fn setup<'a>() -> Setup<'a> {
    let e = Env::default();
    e.mock_all_auths();
    let issuer = Address::generate(&e);
    let sac = e.register_stellar_asset_contract_v2(issuer);
    let admin = Address::generate(&e);
    let keeper = Address::generate(&e);
    let init: Vec<Val> = vec![&e, admin.into_val(&e), keeper.into_val(&e)];
    let id = e.register(ExampleStrategy, (sac.address(), init));
    let strategy = ExampleStrategyClient::new(&e, &id);
    let vault = Address::generate(&e);
    strategy.set_depositor(&vault, &true);
    Setup {
        token: TokenClient::new(&e, &sac.address()),
        minter: StellarAssetClient::new(&e, &sac.address()),
        e,
        strategy,
        keeper,
        vault,
    }
}

#[test]
fn deposit_returns_post_deposit_balance_not_amount() {
    let s = setup();
    s.minter.mint(&s.vault, &1_000_0000000);
    assert_eq!(s.strategy.deposit(&600_0000000, &s.vault), 600_0000000);
    // Second deposit returns the running total, not 400.
    assert_eq!(s.strategy.deposit(&400_0000000, &s.vault), 1_000_0000000);
    assert_eq!(s.strategy.balance(&s.vault), 1_000_0000000);
}

#[test]
fn withdraw_returns_remaining_balance_and_pays_to() {
    let s = setup();
    let user = Address::generate(&s.e);
    s.minter.mint(&s.vault, &1_000_0000000);
    s.strategy.deposit(&1_000_0000000, &s.vault);
    // On a vault withdraw `to` is the end user, not the vault.
    assert_eq!(s.strategy.withdraw(&300_0000000, &s.vault, &user), 700_0000000);
    assert_eq!(s.token.balance(&user), 300_0000000);
    // Full exit leaves nothing behind.
    assert_eq!(s.strategy.withdraw(&700_0000000, &s.vault, &user), 0);
    assert_eq!(s.strategy.balance(&s.vault), 0);
}

#[test]
fn balance_of_unknown_address_is_zero() {
    let s = setup();
    assert_eq!(s.strategy.balance(&Address::generate(&s.e)), 0);
}

#[test]
fn deposit_from_non_allowlisted_fails_closed() {
    let s = setup();
    let stranger = Address::generate(&s.e);
    s.minter.mint(&stranger, &100);
    assert_eq!(
        s.strategy.try_deposit(&100, &stranger),
        Err(Ok(StrategyError::NotAuthorized))
    );
}

#[test]
fn withdraw_still_works_after_depositor_is_delisted() {
    let s = setup();
    s.minter.mint(&s.vault, &1_000);
    s.strategy.deposit(&1_000, &s.vault);
    s.strategy.set_depositor(&s.vault, &false);
    assert_eq!(s.strategy.withdraw(&1_000, &s.vault, &s.vault), 0);
}

#[test]
fn over_withdraw_is_rejected() {
    let s = setup();
    s.minter.mint(&s.vault, &1_000);
    s.strategy.deposit(&1_000, &s.vault);
    assert_eq!(
        s.strategy.try_withdraw(&1_001, &s.vault, &s.vault),
        Err(Ok(StrategyError::InsufficientBalance))
    );
}

#[test]
fn one_unit_deposit_after_large_position_mints_shares() {
    // The vault sends dust (pro-rata remainders, small fees). Rejecting it reverts the
    // vault operation that triggered it.
    let s = setup();
    s.minter.mint(&s.vault, &1_000_000_0000000);
    s.strategy.deposit(&999_999_0000000, &s.vault);
    s.strategy.deposit(&1, &s.vault);
}

#[test]
fn inflation_attack_is_uneconomical() {
    let s = setup();
    let attacker = Address::generate(&s.e);
    s.strategy.set_depositor(&attacker, &true);
    s.minter.mint(&attacker, &10_000_0000000);
    s.minter.mint(&s.vault, &100_0000000);

    let attacker_in: i128 = 1 + 9_999_0000000;
    s.strategy.deposit(&1, &attacker);
    // Donate straight to the strategy to inflate price per share.
    s.token.transfer(&attacker, &s.strategy.address, &9_999_0000000);
    let victim_in: i128 = 100_0000000;
    let victim_out = s.strategy.deposit(&victim_in, &s.vault);

    // The victim loses at most one share's worth of rounding; the attacker forfeits about
    // half the donation to the virtual shares. The attack costs far more than it steals.
    let victim_loss = victim_in - victim_out;
    let attacker_loss = attacker_in - s.strategy.balance(&attacker);
    assert!(victim_loss * 1_000 < attacker_loss, "victim {} attacker {}", victim_loss, attacker_loss);
}

#[test]
fn balance_does_not_overflow_with_18_decimal_amounts() {
    // 10M tokens at 18 decimals. The plain FixedPoint mul-div overflows i128 here.
    let s = setup();
    let big: i128 = 10_000_000 * 10i128.pow(18);
    s.minter.mint(&s.vault, &big);
    s.strategy.deposit(&big, &s.vault);
    assert_eq!(s.strategy.balance(&s.vault), big);
}

#[test]
fn harvest_rejects_non_keeper() {
    let s = setup();
    let impostor = Address::generate(&s.e);
    // Real auth for this call only: the impostor signs, the keeper does not.
    s.e.set_auths(&[]);
    let res = s
        .strategy
        .mock_auths(&[MockAuth {
            address: &impostor,
            invoke: &MockAuthInvoke {
                contract: &s.strategy.address,
                fn_name: "harvest",
                args: (impostor.clone(), None::<Bytes>).into_val(&s.e),
                sub_invokes: &[],
            },
        }])
        .try_harvest(&impostor, &None);
    assert!(res.is_err());
}

#[test]
fn harvest_rejects_malformed_min_out() {
    let s = setup();
    let bad = Bytes::from_array(&s.e, &[1u8; 5]);
    assert_eq!(
        s.strategy.try_harvest(&s.keeper, &Some(bad)),
        Err(Ok(StrategyError::InvalidArgument))
    );
}

#[test]
fn zero_amounts_are_rejected() {
    let s = setup();
    assert_eq!(
        s.strategy.try_deposit(&0, &s.vault),
        Err(Ok(StrategyError::OnlyPositiveAmountAllowed))
    );
    assert_eq!(
        s.strategy.try_withdraw(&0, &s.vault, &s.vault),
        Err(Ok(StrategyError::OnlyPositiveAmountAllowed))
    );
}
