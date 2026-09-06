# Factory Module

> **Living document.** Read this before modifying the module. Update it in the same change whenever the module's behavior, endpoints, files, or dependencies change.

**Source:** `factory/` · **Last verified:** 2026-09-05

Background: [`../defindex-contracts.md`](../defindex-contracts.md) section 2.

## Purpose

One factory per network deploys vaults from a pinned vault WASM hash and stamps every new vault with the protocol-level fee receiver and fee rate. It is the only place the DeFindex protocol fee is centrally set, and it is the registry that indexers and the SDK use to enumerate vaults. Changing the constructor arg order here silently breaks vault deployment, because the args are pushed positionally into an untyped `Vec<Val>`.

## Structure

| File | Purpose |
|---|---|
| `factory/src/lib.rs` | `FactoryTrait` declaration and the `DeFindexFactory` impl. |
| `factory/src/vault.rs` | `create_contract`, the `deploy_v2` wrapper with the salt scheme. |
| `factory/src/storage.rs` | `DataKey`, instance and persistent accessors, TTL bumps. |
| `factory/src/constants.rs` | `MAX_DEFINDEX_FEE = 9000`. |
| `factory/src/error.rs` | `FactoryError`. |
| `factory/src/events.rs` | Event structs and publishers. |
| `factory/src/test.rs`, `factory/src/test/factory/` | Unit tests. |

## Public surface

Contract type `DeFindexFactory` (`factory/src/lib.rs:194`), impl at `factory/src/lib.rs:255`.

| Function | Auth | Notes |
|---|---|---|
| `__constructor(e, admin: Address, defindex_receiver: Address, defindex_fee: u32, vault_wasm_hash: BytesN<32>) -> Result<(), FactoryError>` (`factory/src/lib.rs:271`) | none, constructor | Returns `FeeTooHigh` if `defindex_fee > 9000` (`factory/src/storage.rs:112`). |
| `create_defindex_vault(e, roles: Map<u32, Address>, vault_fee: u32, assets: Vec<AssetStrategySet>, soroswap_router: Address, name_symbol: Map<String, String>, upgradable: bool) -> Result<Address, FactoryError>` (`factory/src/lib.rs:300`) | **none** | Permissionless. Anyone can deploy a vault carrying the DeFindex protocol receiver and rate. |
| `create_defindex_vault_deposit(e, caller: Address, roles, vault_fee, assets, soroswap_router, name_symbol, upgradable, amounts: Vec<i128>) -> Result<Address, FactoryError>` (`factory/src/lib.rs:341`) | `caller.require_auth()` (`factory/src/lib.rs:353`) | Deploys then immediately deposits on the caller's behalf. Returns `AssetLengthMismatch` when `assets.len() != amounts.len()` (`factory/src/lib.rs:355`). |
| `set_new_admin(e, new_admin: Address)` (`factory/src/lib.rs:384`) | `admin.require_auth()` (`factory/src/lib.rs:387`) | Single-step handoff. |
| `set_defindex_receiver(e, new_fee_receiver: Address)` (`factory/src/lib.rs:402`) | `admin.require_auth()` (`factory/src/lib.rs:405`) | Affects only vaults deployed afterwards. |
| `set_defindex_fee(e, defindex_fee: u32)` (`factory/src/lib.rs:420`) | `admin.require_auth()` (`factory/src/lib.rs:423`) | Same, and validated against `MAX_DEFINDEX_FEE`. |
| `set_vault_wasm_hash(e, new_vault_wasm_hash: BytesN<32>)` (`factory/src/lib.rs:430`) | `admin.require_auth()` (`factory/src/lib.rs:433`) | |
| `admin(e) -> Result<Address, FactoryError>` (`factory/src/lib.rs:450`) | none | |
| `defindex_receiver(e) -> Result<Address, FactoryError>` (`factory/src/lib.rs:462`) | none | |
| `total_vaults(e) -> Result<u32, FactoryError>` (`factory/src/lib.rs:474`) | none | Defaults to 0 (`factory/src/storage.rs:58`). |
| `get_vault_by_index(e, index: u32) -> Result<Address, FactoryError>` (`factory/src/lib.rs:488`) | none | `IndexDoesNotExist` when unset (`factory/src/storage.rs:67`). |
| `defindex_fee(e) -> Result<u32, FactoryError>` (`factory/src/lib.rs:501`) | none | |
| `vault_wasm_hash(e) -> Result<BytesN<32>, FactoryError>` (`factory/src/lib.rs:519`) | none | |

## Key methods

- **`create_vault_internal`** (`factory/src/lib.rs:197`): builds the vault constructor argument vector by pushing eight values in a fixed order: assets, roles, vault_fee, defindex_receiver, defindex_fee, soroswap_router, name_symbol, upgradable (`factory/src/lib.rs:211`-`:218`). This is untyped `Vec<Val>`, so a mismatch with `DeFindexVault::__constructor` (`vault/src/lib.rs:115`) fails only at deploy time. Keep the two in lockstep.
- **`create_contract`** (`factory/src/vault.rs:7`): salts the deploy with the big-endian bytes of `total_vaults` left-padded into a 32-byte array (`factory/src/vault.rs:15`-`:21`), then calls `deployer().with_current_contract(salt).deploy_v2(...)`. Vault addresses are therefore deterministic from `(factory address, vault index)`, and the counter must never be rolled back or two deploys would collide.
- **`perform_initial_deposit`** (`factory/src/lib.rs:234`): builds `amounts_min` as all zeros (`factory/src/lib.rs:241`) and invokes `deposit` with `invest = false` (`factory/src/lib.rs:249`). No slippage protection on this path by design; the deposit is the caller's own seed.
- **`add_new_vault`** (`factory/src/storage.rs:70`): writes the vault at index `total_vaults` in persistent storage, bumps its TTL, then increments the counter.

## Storage & TTL

`DataKey` (`factory/src/storage.rs:6`): `Admin`, `DeFindexWasmHash`, `DeFindexReceiver`, `TotalVaults`, `VaultAddressNIndexed(u32)`, `FeeRate`.

- Instance: `Admin`, `DeFindexWasmHash`, `DeFindexReceiver`, `TotalVaults`, `FeeRate`.
- Persistent: `VaultAddressNIndexed(u32)` only (`factory/src/storage.rs:74`), read through `get_persistent_extend_or_error` which bumps TTL on every hit (`factory/src/storage.rs:29`).

TTLs match the vault's (`factory/src/storage.rs:15`-`:20`): instance 30 days with a 29-day threshold, persistent 120 days with a 100-day threshold. `extend_instance_ttl` (`factory/src/storage.rs:22`) runs at the top of every entrypoint.

## Errors

`FactoryError` (`factory/src/error.rs:6`): `NotInitialized` 401, `AssetLengthMismatch` 404, `IndexDoesNotExist` 405, `FeeTooHigh` 406. Note these overlap numerically with `StrategyError::NotInitialized` 401 (`strategies/core/src/error.rs:8`); disambiguate by the contract that raised them.

## Events

Topic prefix `"DeFindexFactory"`:

| Topic | Publisher | Payload |
|---|---|---|
| `create` | `factory/src/events.rs:28` | `CreateDeFindexEvent { roles, vault_fee, assets }` (`factory/src/events.rs:8`) |
| `nadmin` | `factory/src/events.rs:42` | `NewAdminEvent` (`factory/src/events.rs:34`) |
| `nreceiver` | `factory/src/events.rs:58` | `NewDeFindexReceiverEvent` (`factory/src/events.rs:48`) |
| `n_fee` | `factory/src/events.rs:72` | `NewFeeRateEvent` (`factory/src/events.rs:64`) |
| `n_wasm` | `factory/src/events.rs:88` | `NewVaultWasmHashEvent` (`factory/src/events.rs:78`) |

The `create` event does **not** include the new vault address. Consumers must read the transaction return value or `get_vault_by_index(total_vaults - 1)`.

## Dependencies

Crates (`factory/Cargo.toml`): `soroban-sdk`, `defindex-strategy-core`, `common`. The `defindex-strategy-core` dependency is declared but nothing in `factory/src` imports it.

Cross-contract:

- Deploys the **vault** WASM via `deploy_v2` (`factory/src/vault.rs:25`).
- Invokes `deposit` on the freshly created vault (`factory/src/lib.rs:251`).

Called by: `src/deploy_factory.ts` (deploys the factory) and `src/deploy_vault.ts`, which invokes `create_defindex_vault` (`src/deploy_vault.ts:108`). Deployed factory addresses are recorded in `public/mainnet.contracts.json:3` and `public/testnet.contracts.json:3`.

## Gotchas & invariants

- **Vault creation is permissionless.** `create_defindex_vault` has no auth check (`factory/src/lib.rs:300`). Anyone can mint a vault that names the DeFindex protocol receiver and rate. Treat the factory's vault list as unfiltered; do not assume a listed vault is operated by the team.
- **Existing vaults never see fee or receiver updates.** Both are copied into the vault's own instance storage at construction (`factory/src/lib.rs:214`-`:215`). Changing them on the factory only affects future deploys.
- **The vault WASM hash is not verified.** `set_vault_wasm_hash` accepts any 32 bytes; a wrong hash makes every subsequent `create_defindex_vault` fail at deploy time.
- **`FeeRate` and `DeFindexWasmHash` are read with `?` on an `Option`** (`factory/src/storage.rs:48`, `:121`), so an uninitialized factory returns `NotInitialized` rather than panicking. `get_total_vaults` instead defaults to 0 (`factory/src/storage.rs:58`).
- **`factory/Makefile:15` points at a directory that does not exist.** The prerequisite recipe runs `$(MAKE) -C ../defindex build`, but the vault crate lives in `vault/`. If `../target/wasm32v1-none/release/defindex_vault.optimized.wasm` is missing, that rule fails. Build from the repo root (`make build`) or run `make -C vault build` first.
- **`factory/Makefile:21` `clean` deletes the vault WASM** rather than running `cargo clean`, unlike every other crate Makefile.
- **`create_contract` is `#![allow(unused)]`-wrapped** (`factory/src/vault.rs:1`), which suppresses warnings for the unused imports in that file.

## Testing

12 `#[test]` cases across `factory/src/test/factory/` covering initialize, admin, create_defindex, events and budget.

Run with `make -C factory test`, which builds first (`factory/Makefile:6`). The tests `contractimport!` the built vault and hodl-strategy WASMs (`factory/src/test.rs:13`, `:47`), so `make build` from the repo root must have run.
