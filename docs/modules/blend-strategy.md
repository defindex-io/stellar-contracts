# Blend Strategy Module

> **Living document.** Read this before modifying the module. Update it in the same change whenever the module's behavior, endpoints, files, or dependencies change.

**Source:** `strategies/blend/` · **Last verified:** 2026-09-05

Background: [`../defindex-contracts.md`](../defindex-contracts.md) section 5, and `strategies/blend/Readme.md`.

## Purpose

The only production strategy in this repo. It supplies a vault's underlying asset into a Blend V2 lending pool, tracks each vault's position in strategy shares backed by Blend bTokens, and auto-compounds BLND emissions by swapping them back into the underlying through Soroswap. Every mainnet strategy address in `public/mainnet.contracts.json` is an instance of this contract. Its share math is where rounding attacks land, so changes to `reserves.rs` or `utils.rs` need the rounding and inflation tests re-run.

## Structure

| File | Purpose |
|---|---|
| `strategies/blend/src/lib.rs` | `BlendStrategy`, the `DeFindexStrategyTrait` impl, plus keeper getter/setter. |
| `strategies/blend/src/blend_pool.rs` | Blend pool client (`contractimport!` of the vendored pool WASM) and supply/withdraw/claim/reinvest. |
| `strategies/blend/src/reserves.rs` | `StrategyReserves`, the bToken/share conversion math, deposit/withdraw/harvest bookkeeping. |
| `strategies/blend/src/utils.rs` | `shares_to_underlying`, `calculate_optimal_deposit_amount`, `calculate_optimal_withdraw_amount`. |
| `strategies/blend/src/soroswap.rs` | `internal_swap_exact_tokens_for_tokens` against the Soroswap router. |
| `strategies/blend/src/storage.rs` | `Config`, `DataKey`, vault shares, keeper, TTLs. |
| `strategies/blend/src/constants.rs` | `SCALAR_12 = 1_000_000_000_000`. |
| `strategies/blend/src/test.rs`, `strategies/blend/src/test/blend/` | Unit tests including inflation and rounding attack cases. |

## Public surface

Contract type `BlendStrategy` (`strategies/blend/src/lib.rs:30`). `STRATEGY_NAME = "BlendStrategy"` (`strategies/blend/src/lib.rs:27`) is the first topic element of every event.

### DeFindexStrategyTrait (`strategies/blend/src/lib.rs:33`)

| Function | Auth | Notes |
|---|---|---|
| `__constructor(e, asset: Address, init_args: Vec<Val>)` (`strategies/blend/src/lib.rs:76`) | none, constructor | `init_args` order is fixed and positional: `[0] blend_pool_address`, `[1] blend_token`, `[2] soroswap_router`, `[3] reward_threshold: i128`, `[4] keeper: Address` (`strategies/blend/src/lib.rs:77`-`:96`). Missing entries `expect`-panic. Calls the pool's `get_reserve(asset)` to derive `reserve_id` (`strategies/blend/src/lib.rs:100`) and stores `claim_ids = [reserve_id * 2 + 1]`, the bToken emission id (`strategies/blend/src/lib.rs:116`). `reward_threshold` must be positive (`strategies/blend/src/lib.rs:120`). |
| `asset(e) -> Result<Address, StrategyError>` (`strategies/blend/src/lib.rs:146`) | none | |
| `deposit(e, amount: i128, from: Address) -> Result<i128, StrategyError>` (`strategies/blend/src/lib.rs:166`) | `from.require_auth()` (`strategies/blend/src/lib.rs:170`) | Rejects `amount <= 0` with `OnlyPositiveAmountAllowed` (`strategies/blend/src/lib.rs:19`). Returns the caller's post-deposit underlying balance. |
| `harvest(e, from: Address, data: Option<Bytes>) -> Result<(), StrategyError>` (`strategies/blend/src/lib.rs:220`) | `keeper.require_auth()` **and** `from == keeper` (`strategies/blend/src/lib.rs:224`-`:228`) | The only role-gated entrypoint. `data`, when non-empty, is decoded as a 16-byte big-endian `i128` used as `amount_out_min` for the BLND swap (`strategies/blend/src/lib.rs:235`-`:242`); absent or empty data defaults to **0**, meaning no slippage protection. |
| `withdraw(e, amount: i128, from: Address, to: Address) -> Result<i128, StrategyError>` (`strategies/blend/src/lib.rs:271`) | `from.require_auth()` (`strategies/blend/src/lib.rs:275`) | Underlying goes to `to`; shares are burnt from `from`. |
| `balance(e, from: Address) -> Result<i128, StrategyError>` (`strategies/blend/src/lib.rs:309`) | none | Refreshes `b_rate` from the pool before converting, so it reflects accrued interest. Returns 0 when the caller holds no shares. |

### Keeper management (`strategies/blend/src/lib.rs:327`)

| Function | Auth | Notes |
|---|---|---|
| `set_keeper(e, new_keeper: Address) -> Result<(), StrategyError>` (`strategies/blend/src/lib.rs:341`) | current keeper `require_auth()` (`strategies/blend/src/lib.rs:345`) | Emits `(STRATEGY_NAME, "setkeeper")` with `(old_keeper, new_keeper)` (`strategies/blend/src/lib.rs:348`). |
| `get_keeper(e) -> Result<Address, StrategyError>` (`strategies/blend/src/lib.rs:364`) | none | |

## Key methods

- **Three-layer accounting.** Underlying asset ↔ Blend bTokens ↔ strategy shares. `b_rate` converts bTokens to underlying and is read live from the pool (`strategies/blend/src/blend_pool.rs:264`). `total_shares` / `total_b_tokens` convert shares to bTokens (`strategies/blend/src/reserves.rs:26`-`:51`). Interest accrues by `b_rate` rising, so a share is worth more underlying over time without any share count changing.
- **`get_strategy_reserve_updated`** (`strategies/blend/src/reserves.rs:79`): reads the stored reserves and immediately refreshes `b_rate` from the pool. Every entrypoint that touches value calls this first. It does **not** persist the refreshed rate; only `deposit`, `withdraw` and `harvest` write reserves back.
- **`calculate_optimal_deposit_amount`** (`strategies/blend/src/utils.rs:36`): converts the requested amount to bTokens floor, converts those to shares floor, then converts the shares back to bTokens **ceil** and back to underlying **ceil** (`strategies/blend/src/utils.rs:55`, `:64`). The deliberate floor-then-ceil asymmetry means the strategy never mints shares it cannot back. Zero minted shares raise `InvalidSharesMinted` (`strategies/blend/src/utils.rs:51`); zero optimal bTokens raise `BTokensAmountBelowMin` (`strategies/blend/src/utils.rs:60`). The unused remainder is transferred straight back to the caller (`strategies/blend/src/lib.rs:181`).
- **`calculate_optimal_withdraw_amount`** (`strategies/blend/src/utils.rs:71`): the mirror image. bTokens ceil, shares ceil, then bTokens floor and underlying floor. Rounding always favors the pool over the withdrawer.
- **`reserves::deposit`** (`strategies/blend/src/reserves.rs:135`): on the very first deposit (`total_shares == 0`) it mints `new_minted_shares` into `total_shares` but credits the depositor `new_minted_shares - 1000` (`strategies/blend/src/reserves.rs:156`-`:165`). Those 1000 orphan shares are the anti-inflation buffer, and they are permanently unowned. A first deposit minting 1000 or fewer shares panics with `InvalidSharesMinted` (`strategies/blend/src/reserves.rs:158`).
- **`reserves::withdraw`** (`strategies/blend/src/reserves.rs:210`): checks both the global reserve totals and the caller's own share balance before subtracting (`strategies/blend/src/reserves.rs:226`, `:239`), returning `InsufficientBalance` either way.
- **`blend_pool::supply`** (`strategies/blend/src/blend_pool.rs:56`): pre-authorizes exactly one sub-invocation, the underlying `transfer` from the strategy to the pool (`strategies/blend/src/blend_pool.rs:86`), then submits a `RequestType::Supply` request. When `is_reinvest` is true it measures the supply position before and after to derive the real bTokens minted (`strategies/blend/src/blend_pool.rs:66`, `:111`-`:120`); otherwise it returns the raw amount.
- **`blend_pool::perform_reinvest`** (`strategies/blend/src/blend_pool.rs:212`): no-ops and returns current reserves when the BLND balance is below `config.reward_threshold` (`strategies/blend/src/blend_pool.rs:218`). Otherwise it swaps the whole BLND balance to underlying through Soroswap with a deadline of `now + 1` second (`strategies/blend/src/blend_pool.rs:226`-`:230`), re-supplies the proceeds, and folds the new bTokens into `total_b_tokens` without minting shares (`strategies/blend/src/reserves.rs:285`). That is what makes yield accrue to existing shareholders.
- **`soroswap::internal_swap_exact_tokens_for_tokens`** (`strategies/blend/src/soroswap.rs:27`): resolves the pair via `router_pair_for`, pre-authorizes a single transfer to the pair, then calls `swap_exact_tokens_for_tokens`. Router failure panics with `SoroswapPairError` or `InternalSwapError` (`strategies/blend/src/soroswap.rs:51`, `:81`).

## Storage & TTL

`DataKey` (`strategies/blend/src/storage.rs:18`):

| Key | Scope | Contents |
|---|---|---|
| `Config` | instance (`strategies/blend/src/storage.rs:41`) | `Config { asset, pool, reserve_id, blend_token, router, claim_ids, reward_threshold }` (`strategies/blend/src/storage.rs:6`) |
| `Reserves` | instance (`strategies/blend/src/storage.rs:80`) | `StrategyReserves { total_shares, total_b_tokens, b_rate }`, defaulting to all zeros when unset (`strategies/blend/src/storage.rs:89`) |
| `VaultPos(Address)` | persistent (`strategies/blend/src/storage.rs:56`) | That vault's share balance, 7-decimal precision |
| `Keeper` | instance (`strategies/blend/src/storage.rs:97`) | |

TTLs (`strategies/blend/src/storage.rs:25`-`:31`): instance 30 days with a 29-day threshold, persistent 120 days with a 100-day threshold. `extend_instance_ttl` (`strategies/blend/src/storage.rs:33`) runs at the top of every entrypoint.

`set_vault_shares` writes without bumping TTL (`strategies/blend/src/storage.rs:54`); only `get_vault_shares` bumps it (`strategies/blend/src/storage.rs:67`). A position that is written but never read would expire.

## Errors

Uses the shared `StrategyError` (`strategies/core/src/error.rs:6`). The variants this strategy actually raises: `NotInitialized` 401 (missing `Config` or `Keeper`), `InsufficientBalance` 412, `UnderflowOverflow` 413, `ArithmeticError` 414, `DivisionByZero` 415, `InvalidSharesMinted` 416, `OnlyPositiveAmountAllowed` 417, `NotAuthorized` 418, `SoroswapPairError` 423, `BTokensAmountBelowMin` 453, `InternalSwapError` 454. See [`strategy-core.md`](strategy-core.md) for the full enum.

## Events

Emitted through `defindex_strategy_core::event` with `STRATEGY_NAME` as the first topic element:

| Topic | Site | Payload |
|---|---|---|
| `("BlendStrategy", deposit)` | `strategies/blend/src/lib.rs:199` | `DepositEvent { amount: optimal_deposit_amount, from }` |
| `("BlendStrategy", harvest)` | `strategies/blend/src/lib.rs:246` | `HarvestEvent { amount: harvested_blend, from: keeper, price_per_share }` where `price_per_share = shares_to_underlying(SCALAR_12, reserves)` (`strategies/blend/src/lib.rs:251`) |
| `("BlendStrategy", withdraw)` | `strategies/blend/src/lib.rs:291` | `WithdrawEvent { amount: optimal_withdraw_amount, from }` |
| `("BlendStrategy", setkeeper)` | `strategies/blend/src/lib.rs:348` | `(old_keeper, new_keeper)` tuple, not a struct |

The deposit and withdraw events report the **optimal** amount actually moved, not the requested amount.

## Dependencies

Crates (`strategies/blend/Cargo.toml`): `soroban-sdk`, `defindex-strategy-core`, `soroban-fixed-point-math 1.3.0`. Test-only: `sep-40-oracle`, `sep-41-token`.

Cross-contract:

- **Blend V2 pool**, client generated from the vendored WASM at `strategies/external_wasms/blend/pool.wasm` (`strategies/blend/src/blend_pool.rs:15`). Calls: `get_reserve` (`strategies/blend/src/lib.rs:100`, `strategies/blend/src/blend_pool.rs:269`), `get_positions` (`strategies/blend/src/blend_pool.rs:67`), `submit` (`strategies/blend/src/blend_pool.rs:104`, `:124`, `:163`), `claim` (`strategies/blend/src/blend_pool.rs:189`). The vendored WASMs are built from `blend-capital/blend-contracts-v2` commit `a7400fe` (`strategies/external_wasms/blend/README.md:2`).
- **Soroswap router**, address from `Config.router`, invoked by symbol: `router_pair_for` and `swap_exact_tokens_for_tokens` (`strategies/blend/src/soroswap.rs:48`, `:78`).
- **Underlying asset and BLND token** through `TokenClient` (`strategies/blend/src/lib.rs:177`, `strategies/blend/src/blend_pool.rs:215`).

Called by: DeFindex vaults through `DeFindexStrategyClient`. Deployed by `src/strategies/deploy_blend.ts`, `src/strategies/multi_deploy_blend.ts`, and `bash/deploy-blend-strategy.sh`.

## Gotchas & invariants

- **Harvest has no slippage protection by default.** `data = None` or empty bytes means `amount_out_min = 0` (`strategies/blend/src/lib.rs:241`), and the swap deadline is `now + 1` second (`strategies/blend/src/blend_pool.rs:229`). The keeper must pass a 16-byte big-endian `i128` to protect the BLND swap. A sandwich on an unprotected harvest steals yield from every depositor.
- **`data` must be exactly 16 bytes when present.** `copy_into_slice` into a `[u8; 16]` (`strategies/blend/src/lib.rs:238`) panics on any other non-empty length.
- **The first depositor loses 1000 shares.** This mirrors the vault's `MINIMUM_LIQUIDITY`, but at the strategy layer, and it is why the deploy scripts make a bootstrap deposit right after deploying (`README.md:229`).
- **`reserve_id` is frozen at construction.** If the Blend pool ever reindexes its reserves, the stored `reserve_id` and `claim_ids` become wrong and harvest claims the wrong emissions. There is no setter.
- **`claim_ids` only covers the bToken emission** (`reserve_id * 2 + 1`, `strategies/blend/src/lib.rs:116`). dToken emissions are never claimed, which is correct for a supply-only strategy.
- **`reward_threshold` is set at construction and cannot be changed.** The TypeScript deploy path hardcodes `40` (`src/strategies/deploy_blend.ts:90`, `src/utils/deploy_tools.ts:89`) and ignores the `reward_threshold` field in `blend_deploy_config.json`. The bash script does read it as an argument (`bash/deploy-blend-strategy.sh:175`).
- **`perform_reinvest` reads the strategy's whole BLND balance** (`strategies/blend/src/blend_pool.rs:214`), so any BLND sent to the contract by anyone gets swapped and folded into reserves on the next harvest.
- **`b_rate` is refreshed but not always persisted.** `get_strategy_reserve_updated` returns an updated copy; reserves are only written back in `reserves::deposit`, `reserves::withdraw` and `reserves::harvest`. A pure `balance()` call leaves storage untouched, which is what makes it a safe read.
- **`shares_to_b_tokens_down` divides by `total_shares` with no zero guard** (`strategies/blend/src/reserves.rs:47`); it relies on `shares_to_underlying` short-circuiting when totals are zero (`strategies/blend/src/utils.rs:26`). Any new caller must keep that guard.
- **The strategy is single-asset and single-pool.** `asset` and `pool` are both fixed at construction. One strategy contract per (asset, Blend pool) pair, which is why `public/mainnet.contracts.json` lists one per asset per pool.

## Testing

21 `#[test]` cases across `strategies/blend/src/test/blend/`, including `inflation_attack.rs`, `rounding_attack.rs`, `min_amount_out.rs`, `reserves.rs`, `error.rs`, `soroswap.rs` and `success.rs`.

Run with `make -C strategies/blend test`. The tests spin up a full local Blend deployment from the vendored WASMs (comet, backstop, emitter, pool_factory, pool at `strategies/blend/src/test.rs:22`-`:76`) plus a Soroswap factory/router/pair from `strategies/external_wasms/soroswap/` (`strategies/blend/src/test/blend/soroswap_setup.rs:4`). Nothing here reads from `target/`, so this crate's tests do not need a prior workspace build.
