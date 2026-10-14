//! Drives the real DeFindex vault WASM against this strategy. A fake `vault` address
//! cannot catch return-value bugs: here the vault's own reports read what you return.
//! Most tests use `mock_all_auths` for setup, which hides auth bugs. The real-auth test
//! `deposit_pull_matches_the_vaults_single_preauthorized_transfer` does not: pull any
//! amount other than exactly `amount` and it fails with Error(Auth, InvalidAction).
//!
//! Build the vault first, from the stellar-contracts repo root:
//!   cd vault && make build
//! Then: cargo test --features vault-it
//! Outside this repo, fetch the vault WASM you will actually integrate with
//! (`stellar contract fetch --id <vault> -o defindex_vault.wasm`) and point the path at it.

extern crate std;

use crate::{ExampleStrategy, ExampleStrategyClient};
use soroban_sdk::{
    testutils::Address as _,
    token::{StellarAssetClient, TokenClient},
    vec, Address, BytesN, Env, IntoVal, Map, String, Val, Vec,
};

mod vault {
    soroban_sdk::contractimport!(
        file = "../../../target/wasm32v1-none/release/defindex_vault.optimized.wasm"
    );
}

struct World<'a> {
    e: Env,
    vault: vault::Client<'a>,
    strategy: ExampleStrategyClient<'a>,
    token: TokenClient<'a>,
    minter: StellarAssetClient<'a>,
    manager: Address,
}

fn world<'a>() -> World<'a> {
    let e = Env::default();
    e.mock_all_auths();
    e.cost_estimate().budget().reset_unlimited();

    let sac = e.register_stellar_asset_contract_v2(Address::generate(&e));
    let admin = Address::generate(&e);
    let init: Vec<Val> = vec![&e, admin.into_val(&e), Address::generate(&e).into_val(&e)];
    let strategy_id = e.register(ExampleStrategy, (sac.address(), init));
    let strategy = ExampleStrategyClient::new(&e, &strategy_id);

    // Allowlist the vault by WASM hash, the way production does it: every vault built from
    // this code may deposit, no per-address curation. Upload first to learn the hash.
    let vault_hash: BytesN<32> = e.deployer().upload_contract_wasm(vault::WASM);
    strategy.set_allowed_hash(&vault_hash, &true);

    let manager = Address::generate(&e);
    // Role ids: 0 EmergencyManager, 1 VaultFeeReceiver, 2 Manager, 3 RebalanceManager.
    let mut roles: Map<u32, Address> = Map::new(&e);
    roles.set(0u32, manager.clone());
    roles.set(1u32, Address::generate(&e));
    roles.set(2u32, manager.clone());
    roles.set(3u32, manager.clone());
    let mut name_symbol: Map<String, String> = Map::new(&e);
    name_symbol.set(String::from_str(&e, "name"), String::from_str(&e, "Example"));
    name_symbol.set(String::from_str(&e, "symbol"), String::from_str(&e, "EXV"));

    let assets = vec![
        &e,
        vault::AssetStrategySet {
            address: sac.address(),
            strategies: vec![
                &e,
                vault::Strategy {
                    address: strategy_id.clone(),
                    name: String::from_str(&e, "ExampleStrategy"),
                    paused: false,
                },
            ],
        },
    ];
    let vault_id = e.register(
        vault::WASM,
        (
            assets,
            roles,
            1_000u32, // vault fee, bps
            Address::generate(&e),
            500u32, // protocol share of the fee, bps
            Address::generate(&e), // Soroswap router, unused without swap instructions
            name_symbol,
            false,
        ),
    );

    World {
        vault: vault::Client::new(&e, &vault_id),
        strategy,
        token: TokenClient::new(&e, &sac.address()),
        minter: StellarAssetClient::new(&e, &sac.address()),
        manager,
        e,
    }
}

#[test]
fn vault_invests_reports_and_withdraws_through_strategy() {
    let w = world();
    let user = Address::generate(&w.e);
    w.minter.mint(&user, &1_000_0000000);

    w.vault.deposit(&vec![&w.e, 1_000_0000000], &vec![&w.e, 0], &user, &false);
    w.vault.rebalance(
        &w.manager,
        &vec![&w.e, vault::Instruction::Invest(w.strategy.address.clone(), 800_0000000)],
    );
    assert_eq!(w.strategy.balance(&w.vault.address), 800_0000000);

    // The vault's report holds what deposit() returned; no phantom gain or loss.
    let report = w.vault.report().get(0).unwrap();
    assert_eq!(report.prev_balance, 800_0000000);
    assert_eq!(report.gains_or_losses, 0);

    // Withdraw more than idle so the vault must unwind from the strategy straight to user.
    let shares = w.vault.balance(&user);
    w.vault.withdraw(&shares, &vec![&w.e, 0], &user);
    // The vault keeps MINIMUM_LIQUIDITY (1000) shares of its own from the first deposit,
    // so 1000 units stay invested even after every user exits.
    assert_eq!(w.strategy.balance(&w.vault.address), 1_000);
    assert_eq!(w.token.balance(&user), 1_000_0000000 - 1_000);
}

#[test]
fn yield_is_seen_as_gain_and_fees_lock() {
    let w = world();
    let user = Address::generate(&w.e);
    w.minter.mint(&user, &1_000_0000000);
    w.vault.deposit(&vec![&w.e, 1_000_0000000], &vec![&w.e, 0], &user, &false);
    w.vault.rebalance(
        &w.manager,
        &vec![&w.e, vault::Instruction::Invest(w.strategy.address.clone(), 1_000_0000000)],
    );

    // Simulate yield: underlying grows inside the strategy. Your balance() must rise.
    w.minter.mint(&w.strategy.address, &100_0000000);
    // lock_fees only locks gains already recorded. report() is what reads balance() and
    // records the delta (deposit, withdraw and get_asset_amounts_per_shares also do).
    let gain = w.vault.report().get(0).unwrap().gains_or_losses;
    assert!(gain >= 100_0000000 - 1 && gain <= 100_0000000);
    let reports = w.vault.lock_fees(&None);
    let report = reports.get(0).unwrap();
    // 10% vault fee on ~100 of gain is locked; rounding may shave a unit.
    assert!(report.locked_fee >= 10_0000000 - 1 && report.locked_fee <= 10_0000000);
}

#[test]
fn deposit_pull_matches_the_vaults_single_preauthorized_transfer() {
    // Real auth for the rebalance: only the manager signs, nothing is mocked for the
    // strategy's token pull. It passes only because the vault pre-authorizes exactly
    // `transfer(vault, strategy, amount)` and the strategy makes exactly that call.
    let w = world();
    let user = Address::generate(&w.e);
    w.minter.mint(&user, &1_000_0000000);
    w.vault.deposit(&vec![&w.e, 1_000_0000000], &vec![&w.e, 0], &user, &false);

    w.e.set_auths(&[]);
    let instructions = vec![
        &w.e,
        vault::Instruction::Invest(w.strategy.address.clone(), 800_0000000),
    ];
    w.vault
        .mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: &w.manager,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &w.vault.address,
                fn_name: "rebalance",
                args: (w.manager.clone(), instructions.clone()).into_val(&w.e),
                sub_invokes: &[],
            },
        }])
        .rebalance(&w.manager, &instructions);
    assert_eq!(w.strategy.balance(&w.vault.address), 800_0000000);
}
