# Common Module

> **Living document.** Read this before modifying the module. Update it in the same change whenever the module's behavior, endpoints, files, or dependencies change.

**Source:** `common/` · **Last verified:** 2026-09-05

## Purpose

A 39-line `no_std` crate holding the two contract types that the vault and the factory must agree on, plus one string helper. It is tiny but load-bearing: `AssetStrategySet` is the shape of the vault's constructor argument and of the factory's `create` event, so any field change here is a wire-format break across both contracts and every off-chain consumer that decodes them.

## Structure

| File | Purpose |
|---|---|
| `common/src/lib.rs` | Module index. `#![no_std]`, exports `models` and `utils`. |
| `common/src/models.rs` | `Strategy` and `AssetStrategySet`. |
| `common/src/utils.rs` | `StringExtensions::concat`. |

## Public surface

| Item | Definition | Notes |
|---|---|---|
| `struct Strategy { address: Address, name: String, paused: bool }` | `common/src/models.rs:5` | `#[contracttype]`. `paused` is mutated in place by the vault (`vault/src/strategies.rs:80`, `:111`). |
| `struct AssetStrategySet { address: Address, strategies: Vec<Strategy> }` | `common/src/models.rs:13` | `#[contracttype]`. One underlying asset and every strategy mounted against it. |
| `trait StringExtensions { fn concat(&self, e: &Env, other: String) -> String }` | `common/src/utils.rs:4`, impl at `:9` | Concatenates two `soroban_sdk::String`s. |

## Key methods

- **`StringExtensions::concat`** (`common/src/utils.rs:10`): its one job is building the vault token name, `"DeFindex-Vault-" + name` (`vault/src/lib.rs:133`-`:135`). It copies both strings into a fixed `[u8; 35]` buffer and **silently truncates at 35 bytes** (`common/src/utils.rs:13`). The prefix is 15 bytes, so a vault name longer than 20 bytes is cut without error. It also calls `core::str::from_utf8(...).unwrap()` (`common/src/utils.rs:19`), so truncating mid-codepoint panics on a multibyte name.

## Dependencies

`soroban-sdk` only (`common/Cargo.toml:11`). Consumed by `vault` and `factory` through the workspace path dependency at `Cargo.toml:20`. `publish = false` (`common/Cargo.toml:7`), so it is not available to external strategy authors; only `defindex-strategy-core` is.

## Gotchas & invariants

- **The 35-byte cap is silent.** No error, no event, no length validation upstream. `vault/src/lib.rs:134` reads the `name` entry from the `name_symbol` map and passes it straight through. Deploy tooling should keep vault names short; `configs.json` uses `"Defindex Vault"` (14 bytes), which fits.
- **Multibyte names can panic.** A name whose 35-byte truncation splits a UTF-8 sequence hits the `unwrap` at `common/src/utils.rs:19` and aborts the constructor.
- **`Strategy.name` is free-form and never validated.** The vault stores whatever the deployer supplies and never reads it for logic, only for display.
- **`AssetStrategySet` is re-exported through generated clients** (`integration-test/src/vault.rs:11`), so downstream code sometimes sees it as `VaultAssetStrategySet`. Same type, different alias.

## Testing

No tests in this crate. `concat` is exercised indirectly by the vault's initialize tests.
