# Integration Test Module

> **Living document.** Read this before modifying the module. Update it in the same change whenever the module's behavior, endpoints, files, or dependencies change.

**Source:** `integration-test/` · **Last verified:** 2026-09-05

## Purpose

A non-publishable crate that wires the real built WASM of factory, vault and strategies together in a Soroban test environment and exercises them end to end. It is the only place the whole protocol is tested as a unit, and the only place resource limits are checked against Stellar's published ledger budgets. It is deliberately not a contract: `integration-test/Makefile:9` prints "No need to build integration tests".

## Structure

| File | Purpose |
|---|---|
| `integration-test/src/test.rs` | `IntegrationTest::setup`, `EnvTestUtils` ledger helpers, and the test module tree. |
| `integration-test/src/setup.rs` | Three prebuilt fixtures: hodl, fixed-APR and Blend single-asset vaults. |
| `integration-test/src/setup/blend_setup.rs` | Deploys a full local Blend V2 stack from vendored WASM. |
| `integration-test/src/setup/soroswap_setup.rs` | Deploys a local Soroswap factory, router and pair. |
| `integration-test/src/factory.rs` | Factory client from the built WASM plus `create_factory_contract`. |
| `integration-test/src/vault.rs` | Vault client from the built WASM plus type re-exports. |
| `integration-test/src/blend_strategy.rs`, `hodl_strategy.rs`, `fixed_strategy.rs` | Strategy clients and deploy helpers. |
| `integration-test/src/token.rs` | Test token helper. |
| `integration-test/src/test/vault_one_hodl_strategy/` | Deposit, withdraw, invest, fee-performance cases. |
| `integration-test/src/test/vault_one_fixed_strategy/` | Deposit, withdraw, fee-performance cases. |
| `integration-test/src/test/vault_blend_strategy/` | Blend end-to-end and `rounding_attack.rs`. |
| `integration-test/src/test/limits/` | Resource-budget ceilings for many-asset and many-strategy vaults. |

## Public surface

Library crate, no contract. The pieces other tests build on:

| Item | Definition | Notes |
|---|---|---|
| `IntegrationTest { env, factory_contract, admin, defindex_receiver, defindex_fee }` | `integration-test/src/test.rs:17` | `setup()` (`:80`) uploads the vault WASM, deploys a factory, then resets the budget to unlimited (`:100`). |
| `trait EnvTestUtils { jump, jump_time, set_default_info }` | `integration-test/src/test.rs:25`, impl `:38` | `jump` assumes 5 seconds per ledger (`:41`). `set_default_info` pins timestamp 1441065600 and sequence 100 (`:67`). All three set `protocol_version: 22`. |
| `create_vault_one_asset_hodl_strategy()` | `integration-test/src/setup.rs:54` | Returns `VaultOneAseetHodlStrategy` (`:18`). |
| `create_vault_one_asset_fixed_strategy()` | `integration-test/src/setup.rs:154` | Returns `VaultOneAseetFixedStrategy` (`:141`). |
| `create_vault_one_blend_strategy()` | `integration-test/src/setup.rs:263` | Returns `VaultOneBlendStrategy` (`:244`). Pulls in the whole Blend and Soroswap setup. |
| `check_limits(e, message)` | `integration-test/src/test/limits/mod.rs:15` | Asserts CPU, memory and read/write entry and byte usage against the constants at `:8`-`:13`. |

Constants: `ONE_YEAR_IN_SECONDS = 31_536_000`, `DEFINDEX_FEE = 50` (0.5%), `DAY_IN_LEDGERS = 17280` (`integration-test/src/test.rs:13`-`:15`), `VAULT_FEE = 100` (1%) (`integration-test/src/setup.rs:31`), `MINIMUM_LIQUIDITY = 1000` mirrored from the vault (`integration-test/src/vault.rs:15`).

## Dependencies

Crates (`integration-test/Cargo.toml`): `soroban-sdk` with `testutils`, `defindex-factory` and `defindex-vault` as path dependencies, `hodl_strategy` as a path dependency, `common`, `soroban-fixed-point-math`, `sep-40-oracle` and `sep-41-token`.

WASM loaded through `contractimport!`, all of which must already exist:

| Artifact | Source |
|---|---|
| `defindex_factory.optimized.wasm` | built (`integration-test/src/factory.rs:2`) |
| `defindex_vault.optimized.wasm` | built (`integration-test/src/vault.rs:3`) |
| `blend_strategy.optimized.wasm` | built (`integration-test/src/blend_strategy.rs:3`) |
| `hodl_strategy.optimized.wasm`, `fixed_apr_strategy.optimized.wasm` | built (`integration-test/src/hodl_strategy.rs:3`, `integration-test/src/fixed_strategy.rs:3`) |
| Blend comet, backstop, emitter, pool_factory, pool | vendored (`integration-test/src/setup/blend_setup.rs:15`, `:18`, `:50`, `:56`, `:71`) |
| Soroswap pair, factory, router | vendored (`integration-test/src/setup/soroswap_setup.rs:7`, `:13`, `:28`) |

## Gotchas & invariants

- **`cargo test` here fails without a prior workspace build.** Five of the imports point at `../target/wasm32v1-none/release/*.optimized.wasm`. Run `make build` from the repo root first, or `make test` which chains build then test per subdir (`Makefile:12`).
- **The crate declares both a path dependency and a WASM import for the same contracts.** `defindex-factory` and `defindex-vault` are Cargo dependencies (`integration-test/Cargo.toml`) *and* imported as built WASM. Only the WASM path is used by the tests; the Cargo dependencies exist to make Cargo rebuild ordering work.
- **`setup()` resets the budget to unlimited** (`integration-test/src/test.rs:100`). Any test that wants to measure cost must re-arm the budget itself, which is exactly what `test/limits/` does.
- **`integration-test/src/test/limits/mod.rs:4` has `rebalance` commented out**, so `test/limits/rebalance.rs` is dead code that is never compiled.
- **Test modules are gated with `#[allow(unused)]` and not `#[cfg(test)]`** (`integration-test/src/test.rs:121`-`:130`), and `integration-test/src/lib.rs:1` sets `#![allow(clippy::all)]`. Lints will not catch much here.
- **The Blend fixture is expensive.** `create_vault_one_blend_strategy` stands up comet, backstop, emitter, a pool factory, a pool and a Soroswap AMM. Blend-suite runtimes are much longer than the hodl or fixed-APR suites.
- **`protocol_version: 22` is hardcoded** in all three `EnvTestUtils` methods (`integration-test/src/test.rs:42`, `:56`, `:70`). Bumping the Soroban SDK past protocol 22 means updating these.
- **Struct names carry a typo**, `VaultOneAseetHodlStrategy` and `VaultOneAseetFixedStrategy` (`integration-test/src/setup.rs:18`, `:141`). Match it exactly when importing.

## Testing

51 `#[test]` cases across 13 files. Run with `make -C integration-test test` (`integration-test/Makefile:5`) or `cargo test -p integration-test` once the workspace WASM is built.

Coverage by area: hodl-strategy vault (deposit, withdraw, invest, fee performance), fixed-APR vault (deposit, withdraw, fee performance), Blend vault (`blend.rs`, `rounding_attack.rs`), and resource limits (`asset_n_strategies.rs`, `n_asset_one_strategy.rs`). Not covered here: the Soroswap and Xycloans prototype strategies, the vault's Soroswap rebalance swaps (covered in `vault/src/test/vault/router.rs` instead), and the upgrade path (covered in `vault/src/test/vault/upgrade.rs`).
