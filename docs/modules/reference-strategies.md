# Reference Strategies Module

> **Living document.** Read this before modifying the module. Update it in the same change whenever the module's behavior, endpoints, files, or dependencies change.

**Source:** `strategies/hodl/`, `strategies/unsafe_hodl/`, `strategies/fixed_apr/`, `strategies/soroswap/`, `strategies/xycloans/` · **Last verified:** 2026-09-05

Background: [`../defindex-contracts.md`](../defindex-contracts.md) section 6. The production strategy lives in [`blend-strategy.md`](blend-strategy.md); the interface they all implement is in [`strategy-core.md`](strategy-core.md).

## Purpose

Five non-production `DeFindexStrategyTrait` implementations. Hodl, UnsafeHodl and Fixed APR are test fixtures used by the vault and integration test suites. Soroswap and Xycloans are unfinished prototypes with hardcoded addresses and no tests. **None of these appear in `public/mainnet.contracts.json`.** Group them here rather than one doc each because they share the same shape and the same warning: do not put them in front of real money.

## Structure

| Crate | Package name | Contract type | Src lines (non-test) | Tests |
|---|---|---|---|---|
| `strategies/hodl/` | `hodl_strategy` | `HodlStrategy` (`strategies/hodl/src/lib.rs:26`) | 173 | 8 |
| `strategies/unsafe_hodl/` | `unsafe_hodl_strategy` | `HodlStrategy` (`strategies/unsafe_hodl/src/lib.rs:26`) | 173 | 8 |
| `strategies/fixed_apr/` | `fixed_apr_strategy` | `FixAprStrategy` (`strategies/fixed_apr/src/lib.rs:32`) | 302 | 10 |
| `strategies/soroswap/` | `soroswap_strategy` | `SoroswapAdapter` (`strategies/soroswap/src/lib.rs:27`) | 285 | none |
| `strategies/xycloans/` | `xycloans_adapter` | `XycloansAdapter` (`strategies/xycloans/src/lib.rs:28`) | 461 | none |

The Cargo package name is what the build produces, so the WASM path is `target/wasm32v1-none/release/<package>.optimized.wasm`. Note `soroswap_strategy` builds to `soroswap_strategy.optimized.wasm`, but the deploy tooling looks for `soroswap_adapter.optimized.wasm` (`src/utils/contract.ts:29`). See Gotchas.

## Public surface

All five implement the same trait, so only the deltas are listed.

### Hodl (`strategies/hodl/src/lib.rs:29`)

| Function | Auth | Behavior |
|---|---|---|
| `__constructor(e, asset, _init_args)` (`:30`) | none | Stores the underlying asset. `init_args` is ignored, so deploys pass an empty vector. |
| `asset(e)` (`:34`) | none | |
| `deposit(e, amount, from)` (`:40`) | `from.require_auth()` (`:43`) | Transfers the asset in and credits `from`'s tracked balance. Rejects negative amounts with `NegativeNotAllowed` (`:16`). |
| `harvest(e, from, _data)` (`:61`) | **none** | No-op that emits a zero-amount harvest event for indexer parity. |
| `withdraw(e, amount, from, to)` (`:68`) | `from.require_auth()` (`:69`) | Debits `from`, transfers to `to`. |
| `balance(e, from)` (`:88`) | none | Tracked balance, never accrues. |

Storage `DataKey` (`strategies/hodl/src/storage.rs:6`): `UnderlyingAsset` (instance), `Balance(Address)` (persistent, `strategies/hodl/src/balance.rs:22`). Both TTL constants are the 30-day instance pair, and `balance.rs` reuses them for persistent entries (`strategies/hodl/src/balance.rs:11`).

### UnsafeHodl (`strategies/unsafe_hodl/src/lib.rs:29`)

Byte-for-byte the same `lib.rs` as Hodl apart from a trailing comma, and it reports the same `STARETEGY_NAME = "HodlStrategy"` (`strategies/unsafe_hodl/src/lib.rs:23`). **The one real difference is in `balance.rs`:** the insufficient-balance guard in `spend_balance` is commented out (`strategies/unsafe_hodl/src/balance.rs:40`-`:42`), where Hodl returns `InsufficientBalance` (`strategies/hodl/src/balance.rs:40`-`:42`). So `withdraw` can drive a vault's tracked balance negative. That is the entire point of the crate: it is the adversarial-strategy fixture used to prove the vault does not trust strategy return values.

### Fixed APR (`strategies/fixed_apr/src/lib.rs:35`)

| Function | Auth | Behavior |
|---|---|---|
| `__constructor(e, asset, init_args)` (`:36`) | none | `init_args[0]` is `apr_bps: u32`. Missing arg unwraps to a panic (`:41`). |
| `deposit(e, amount, from)` (`:53`) | `from.require_auth()` (`:56`) | Settles pending yield first, then credits principal and resets the per-user harvest timestamp (`:66`). |
| `harvest(e, from, _data)` (`:77`) | **none** | Moves accrued yield from the yield balance into the principal balance. Anyone can call it for anyone. |
| `withdraw(e, amount, from, to)` (`:95`) | `from.require_auth()` (`:96`) | Does **not** settle yield first, so unharvested yield is left behind. |
| `balance(e, from)` (`:115`) | none | Principal only, excludes unharvested yield. |

Yield is `user_balance * apr_bps * seconds_elapsed / (31_536_000 * 10_000)` (`strategies/fixed_apr/src/lib.rs:121`, constants at `strategies/fixed_apr/src/constants.rs:1`-`:2`). Storage `DataKey` (`strategies/fixed_apr/src/storage.rs:5`): `Initialized`, `UnderlyingAsset`, `Balance(Address)`, `YieldBalance(Address)`, `Apr`, `LastHarvestTime(Address)`. `Balance` and `YieldBalance` are persistent (`strategies/fixed_apr/src/balance.rs`, `strategies/fixed_apr/src/yield_balance.rs:22`); the rest are instance. `Initialized` is declared but never used.

### Soroswap adapter (`strategies/soroswap/src/lib.rs:30`)

Provides liquidity to a single USDC/XLM Soroswap pair. `deposit` swaps half the input to XLM and adds liquidity (`strategies/soroswap/src/lib.rs:98`, `:109`); `withdraw` removes all liquidity and swaps back (`strategies/soroswap/src/lib.rs:175`, `:189`); `balance` prices the LP position against spot reserves (`strategies/soroswap/src/lib.rs:215`-`:245`).

`asset()` returns the **Soroswap router address**, not a token (`strategies/soroswap/src/lib.rs:44`). A vault would reject this strategy at construction because `strategy.asset() != asset.address` (`vault/src/lib.rs:161`).

### Xycloans adapter (`strategies/xycloans/src/lib.rs:31`)

Wraps an Xycloans flash-loan pool. `init_args` order: `[0] soroswap_router`, `[1] soroswap_factory`, `[2] xycloans_pool`, `[3] pool_token`, `[4] token_in` (`strategies/xycloans/src/lib.rs:33`-`:57`). `deposit` swaps `token_in` to `pool_token` then deposits (`strategies/xycloans/src/lib.rs:80`-`:85`) and returns `0`. `withdraw` ignores its `amount` argument and unwinds the entire position (`strategies/xycloans/src/lib.rs:96`-`:118`). `balance` prices shares plus matured fees through a spot Soroswap quote (`strategies/xycloans/src/lib.rs:141`-`:152`). Storage `DataKey` (`strategies/xycloans/src/storage.rs:6`): `SoroswapRouterAddress`, `SoroswapFactoryAddress`, `XycloansPoolAddress`, `Token0`, `Token1`, all instance.

## Dependencies

- Hodl, UnsafeHodl, Fixed APR: `soroban-sdk` and `defindex-strategy-core` only. No external contracts.
- Soroswap adapter: Soroswap router and pair clients generated from `strategies/external_wasms/soroswap/soroswap_router.optimized.wasm` and `soroswap_pair.optimized.wasm` (`strategies/soroswap/src/soroswap_router.rs:1`, `strategies/soroswap/src/soroswap_pair.rs:1`).
- Xycloans adapter: Xycloans pool client from `strategies/external_wasms/xycloans/xycloans_pool.wasm` (`strategies/xycloans/src/xycloans_pool.rs:1`) plus the same Soroswap WASMs (`strategies/xycloans/src/soroswap_router.rs:7`, `:11`).

Consumed by: `vault/src/test.rs` (hodl, unsafe_hodl, fixed_apr, at `:17`, `:30`, `:43`), `factory/src/test.rs:13` (hodl), and `integration-test/` (hodl via a path dependency in `integration-test/Cargo.toml`, fixed_apr and blend via built WASM).

## Gotchas & invariants

- **UnsafeHodl is deliberately broken.** Never deploy it to a network where a vault could mount it. Its name is the only marker; the event name and contract type both still say `HodlStrategy`, so an indexer cannot tell the two apart.
- **`harvest` is unauthenticated on Hodl, UnsafeHodl, Fixed APR, Soroswap and Xycloans.** Only the Blend strategy gates it on a keeper. On Fixed APR that means anyone can force-settle another account's yield.
- **The Soroswap adapter hardcodes token and pair addresses** at `strategies/soroswap/src/lib.rs:55`, `:59`, `:69` (deposit) and `:135`, `:139`, `:149`, `:208` (withdraw and balance). Those are testnet addresses baked into the binary. It is also structurally incompatible with the vault because `asset()` returns the router.
- **Xycloans `withdraw` ignores `amount`.** Any partial-withdraw request unwinds everything (`strategies/xycloans/src/lib.rs:96`). Its `deposit` also returns `0` instead of the caller's balance, violating the trait's return contract (`strategies/core/src/lib.rs:55`).
- **Xycloans and Soroswap price positions with spot reserves.** `get_amount_out` on live reserves (`strategies/xycloans/src/lib.rs:149`) is manipulable inside the same transaction.
- **WASM name mismatch for the Soroswap adapter.** The crate builds `soroswap_strategy.optimized.wasm` (`strategies/soroswap/Cargo.toml:2`) but `src/utils/contract.ts:29` and `src/deploy_adapter.ts:22` look for `soroswap_adapter.optimized.wasm`. `yarn deploy-adapter` would fail today, and there is no such script in `package.json`.
- **`strategies/Makefile:1` builds every subdirectory except `core` and `external_wasms`**, so `make build` compiles the Soroswap and Xycloans prototypes too. A compile break in a prototype blocks the whole build.
- **Only `blend` and `fixed_apr`/`hodl` are wired into the deploy tooling.** `Strategies` enum lists exactly `blend`, `hodl`, `fixed_apr` (`src/utils/deploy_tools.ts:8`), and both deploy scripts have hodl and fixed_apr commented out (`src/strategies/deploy_strategies.ts:20`-`:22`).

## Testing

- Hodl: 8 tests in `strategies/hodl/src/test/hodl/` (deposit, withdraw, events).
- UnsafeHodl: 8 tests in `strategies/unsafe_hodl/src/test/unsafe_hodl/`.
- Fixed APR: 10 tests in `strategies/fixed_apr/src/test/fixed_apr/` (initialize, deposit, withdraw, harvest).
- Soroswap and Xycloans: **no tests at all.**

Run per crate with `make -C strategies/<name> test`, or all at once with `make -C strategies test` (`strategies/Makefile:11`). These crates test themselves in-process with `e.register`, so they do not need a prior workspace build.

Each crate's `Makefile` runs `cargo build --target wasm32v1-none --release` then `stellar contract optimize` on its own package name (for example `strategies/hodl/Makefile:9`-`:11`), which is why the produced filename tracks the Cargo package name rather than the directory name.
