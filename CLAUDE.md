# DeFindex stellar-contracts

## Entry summary

Core DeFindex protocol: Soroban smart contracts (Rust workspace) plus TypeScript and bash deploy tooling. Split out of the `paltalabs/defindex` monorepo (`README.md:3`).
It exposes three things to the rest of the org:
1. **`DeFindexVault`** (`vault/src/lib.rs:56`), a SEP-41 share token that custodies assets and routes them into strategies, and **`DeFindexFactory`** (`factory/src/lib.rs:194`), which deploys vaults from a pinned WASM hash.
2. **`DeFindexStrategyTrait`** (`strategies/core/src/lib.rs:18`), published to crates.io as `defindex-strategy-core` `0.2.0` (`strategies/core/Cargo.toml:2`, `:4`, `:8`). Strategies written in other repos implement this trait; `deposit`/`withdraw` must return the caller's balance in underlying, not the delta.
3. **Deployed addresses**, checked in: mainnet factory and 7 Blend strategies (`public/mainnet.contracts.json`), testnet factory, strategies and vaults (`public/testnet.contracts.json`), retired addresses (`public/deprecated.contracts.json`).
It consumes the Soroswap router (swaps and reward compounding), Blend V2 pools (the only production strategy), and `@stellar/stellar-sdk` for deployment.

## Workspace layout

Cargo workspace members: `common`, `strategies/*`, `vault`, `factory`, `integration-test` (`Cargo.toml:2`). `strategies/external_wasms` is excluded (`Cargo.toml:4`).

| Path | What |
|---|---|
| `vault/` | The vault contract. Most protocol logic. |
| `factory/` | Vault factory. |
| `strategies/core/` | The published strategy interface crate. |
| `strategies/blend/` | Production Blend strategy. |
| `strategies/{hodl,unsafe_hodl,fixed_apr,soroswap,xycloans}/` | Test fixtures and prototypes, not production. |
| `common/` | `Strategy` / `AssetStrategySet` shared contract types. |
| `integration-test/` | Cross-contract Rust suite over built WASM. |
| `src/`, `bash/` | TypeScript and bash deploy and on-chain test tooling. |
| `configs.json`, `public/` | Network config and checked-in deployed addresses. |
| `soroswap/`, `strategies/external_wasms/` | Vendored third-party WASM for tests and clients. |
| `docs/` | Knowledge base, audit PDF, and `docs/modules/`. |

## Build, test, deploy

Needs a Rust toolchain with the `wasm32v1-none` target and the `stellar` CLI on PATH.

- `make build` from the repo root builds `strategies vault factory integration-test` in order (`Makefile:1`, `:7`). Each crate runs `cargo build --target wasm32v1-none --release` then `stellar contract optimize` (`vault/Makefile:8`).
- `make test` builds first, then runs `cargo test` per subdir (`Makefile:12`). The build is **mandatory**: unit and integration tests `contractimport!` the optimized WASM from `target/wasm32v1-none/release/` (`vault/src/test.rs:17`, `integration-test/src/vault.rs:3`).
- `make fmt` runs `cargo fmt --all --check` per subdir (`Makefile:17`).
- Deploy scripts (`package.json:5`-`:19`), each taking `<network>` as the first CLI arg: `deploy-factory`, `deploy-blend`, `deploy-strategies`, `deploy-vault`, `multi-deploy-blend`, `publish-addresses`. On-chain smoke tests: `yarn test <network> -a` (flags at `src/tests/mainTest.ts:137`).
- `bash/deploy-blend-strategy.sh` deploys a mainnet Blend strategy from an already-uploaded WASM hash via the `stellar` CLI.
- Env vars (`.env.example`): `DEPLOYER_SECRET_KEY`, `BLEND_DEPLOYER_SECRET_KEY`, `BLEND_KEEPER_SECRET_KEY`, `SOROSWAP_MINT_SECRET_KEY`, `TEST_USER`, `MAINNET_RPC_URL`.
- `package.json:6`'s `yarn build` calls `docker-compose`, but no compose file exists in this repo. Use `make build`.

## Key entry points

- `DeFindexVault::deposit` / `withdraw` / `rebalance` / `distribute_fees` (`vault/src/lib.rs:231`, `:299`, `:846`, `:1039`).
- Vault roles: Manager, EmergencyManager, RebalanceManager, VaultFeeReceiver (`vault/src/access.rs:6`). Enforced by `require_role` / `require_any_role` (`vault/src/access.rs:43`, `:52`).
- `DeFindexFactory::create_defindex_vault` (`factory/src/lib.rs:300`), which is **permissionless**.
- `BlendStrategy::harvest`, keeper-gated (`strategies/blend/src/lib.rs:220`).

## Gotchas

- Contract error codes overlap across contracts: vault `ContractError` uses 100-202 (`vault/src/error.rs:7`), `StrategyError` 401-455 (`strategies/core/src/error.rs:6`), `FactoryError` 401-406 (`factory/src/error.rs:6`). Disambiguate by the emitting contract.
- `get_asset_amounts_per_shares` and any path calling `fetch_total_managed_funds(&e, true)` mutate reports and lock fees. Reads are not free or pure (`vault/src/lib.rs:594`).
- Vault assets, strategies, protocol fee receiver and upgradability are fixed at construction. There is no setter for any of them.
- Blend `harvest` defaults to `amount_out_min = 0` when no `data` is passed (`strategies/blend/src/lib.rs:241`). Keepers must supply a 16-byte big-endian `i128`.
- `unsafe_hodl_strategy` has its balance check commented out on purpose (`strategies/unsafe_hodl/src/balance.rs:40`). Test fixture only, never deploy it.
- `src/publish_addresses.ts:16` and `bash/deploy-blend-strategy.sh:48` still use pre-split monorepo paths and do not resolve here.
- The `README.md` build and deploy instructions still say `apps/contracts/` and reference a docker workspace that is not in this repo.
- `node_modules/` is not in `.gitignore`.

## Cross-repo dependencies

| Direction | Target | What | Evidence |
|---|---|---|---|
| calls | **Soroswap router** (Soroswap) | Vault rebalance swaps: `router_pair_for`, `swap_exact_tokens_for_tokens`, `swap_tokens_for_exact_tokens` | `vault/src/router.rs:45`, `:70`, `:139`; router address stored at construction `vault/src/lib.rs:149` |
| calls | **Soroswap router** (Soroswap) | Blend strategy swaps BLND rewards to the underlying asset | `strategies/blend/src/soroswap.rs:48`, `:78` |
| depends on | **`soroswap-library` crate** (Soroswap) | `get_amount_in`, `get_reserves_with_pair`, and `SoroswapLibraryError` mapping | `Cargo.toml:18`; `vault/src/router.rs:8`; `vault/src/error.rs:2` |
| vendors | **Soroswap factory / router / pair WASM** (Soroswap) | Test fixtures and generated clients | `soroswap/*.wasm` used at `vault/src/test/soroswap_setup.rs:7`; `strategies/external_wasms/soroswap/*` used at `strategies/soroswap/src/soroswap_router.rs:1` |
| calls | **Blend V2 pools** (`blend-capital/blend-contracts-v2`) | `get_reserve`, `get_positions`, `submit`, `claim` from the production strategy | `strategies/blend/src/blend_pool.rs:15`, `:67`, `:104`, `:189`; WASM provenance at `strategies/external_wasms/blend/README.md:2` |
| calls | **Xycloans pool** | Prototype strategy only, not deployed | `strategies/xycloans/src/xycloans_pool.rs:1` |
| published to | **crates.io `defindex-strategy-core` 0.2.0** | The strategy interface other repos implement | `strategies/core/Cargo.toml:2`, `:4`, `:8` |
| origin | **`paltalabs/defindex` monorepo** | This repo was split out of it; still the declared Cargo `repository` | `README.md:3`; `Cargo.toml:12` |
| deployed | **Stellar mainnet / testnet** | Checked-in factory, strategy and vault addresses consumed by any SDK, API or frontend that reads DeFindex on-chain state | `public/mainnet.contracts.json`, `public/testnet.contracts.json`, `public/deprecated.contracts.json` |
| depends on | **`@stellar/stellar-sdk` ^14.0.0** | All deploy and on-chain test tooling | `package.json:35` |

No DeFindex SDK, API or frontend repository is referenced from this codebase. Consumers of the contract interface are not named in source here.

## Module Documentation Convention (MANDATORY)

Every module has a living doc at `docs/modules/<module>.md`. [`docs/modules/README.md`](docs/modules/README.md) is the index that routes a module's source path to its doc. These are the fast on-ramp for anyone, human or agent, touching a module.

**Progressive disclosure, do NOT load all docs at once.** When you are about to touch a module, open `docs/modules/README.md`, find the ONE doc matching the code you are changing, and read only that. Never pull the whole `docs/modules/` folder into context.

**The workflow rule:**
1. **Before modifying a module, read its `docs/modules/<module>.md` first.** It holds the file map, key methods with `file:line`, dependencies, and gotchas.
2. **After modifying a module, update its doc in the same change.** New or removed entrypoints, changed auth, new storage keys, new errors or events, new gotchas, dependency changes. Bump the "Last verified" date.
3. Doc claims must be verified against source and cite `file:line`. Never document something you have not confirmed exists. Write `_TBD, unverified_` rather than guessing.
4. **Adding a new module?** Create its `docs/modules/<module>.md` and add a row to `docs/modules/README.md` in the same change.

Docs follow a shared template: Purpose, Structure, Public surface, Key methods (`file:line`), Storage and TTL, Errors, Events, Dependencies, Gotchas and invariants, Testing.
