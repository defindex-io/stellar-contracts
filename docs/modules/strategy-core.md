# Strategy Core Module

> **Living document.** Read this before modifying the module. Update it in the same change whenever the module's behavior, endpoints, files, or dependencies change.

**Source:** `strategies/core/` · **Last verified:** 2026-09-05

Background: [`../defindex-contracts.md`](../defindex-contracts.md) section 4, and `strategies/core/README.md`.

## Purpose

This is the published interface contract between DeFindex vaults and every strategy, including strategies written in other repositories. It defines `DeFindexStrategyTrait`, the `StrategyError` enum, and the three events a compliant strategy must emit. It is the only crate in this workspace with `publish = true` (`strategies/core/Cargo.toml:8`), version `0.2.0` (`strategies/core/Cargo.toml:4`), so it is consumed from crates.io by external strategy authors. Any breaking change here is a breaking change for every deployed strategy and for the vault, which generates its client from this trait.

## Structure

| File | Purpose |
|---|---|
| `strategies/core/src/lib.rs` | `DeFindexStrategyTrait` plus the `#[contractclient]` that generates `DeFindexStrategyClient`. |
| `strategies/core/src/error.rs` | `StrategyError`. |
| `strategies/core/src/event.rs` | `DepositEvent`, `HarvestEvent`, `WithdrawEvent` and their publishers. |
| `strategies/core/README.md` | Integration guide for external strategy authors. |

## Public surface

`DeFindexStrategyTrait` (`strategies/core/src/lib.rs:18`). The attributes at `strategies/core/src/lib.rs:9`-`:10` generate the `Spec` type and the `DeFindexStrategyClient` that the vault uses (`vault/src/strategies.rs:14`).

| Function | Meaning |
|---|---|
| `__constructor(env: Env, asset: Address, init_args: Vec<Val>)` (`strategies/core/src/lib.rs:27`) | `asset` is the underlying token. Everything else is passed positionally and untyped in `init_args`, so each strategy defines its own arg order. |
| `asset(env: Env) -> Result<Address, StrategyError>` (`strategies/core/src/lib.rs:40`) | The vault calls this at construction and rejects the strategy if it does not match the asset (`vault/src/lib.rs:161`). |
| `deposit(env: Env, amount: i128, from: Address) -> Result<i128, StrategyError>` (`strategies/core/src/lib.rs:57`) | **Must return `from`'s post-deposit balance in underlying**, not the amount deposited. The doc comment at `strategies/core/src/lib.rs:55` states this explicitly and the vault's accounting depends on it (`vault/src/strategies.rs:172`). |
| `harvest(env: Env, from: Address, data: Option<Bytes>) -> Result<(), StrategyError>` (`strategies/core/src/lib.rs:72`) | Claim and/or reinvest rewards. `data` is strategy-defined opaque bytes. |
| `balance(env: Env, from: Address) -> Result<i128, StrategyError>` (`strategies/core/src/lib.rs:90`) | Must be denominated in the **underlying asset**, not in strategy shares (`strategies/core/src/lib.rs:87`). |
| `withdraw(env: Env, amount: i128, from: Address, to: Address) -> Result<i128, StrategyError>` (`strategies/core/src/lib.rs:108`) | Same return contract as `deposit`: `from`'s post-withdraw balance in underlying. |

## Errors

`StrategyError` (`strategies/core/src/error.rs:6`), `#[repr(u32)]`:

| Range | Variants |
|---|---|
| Initialization | `NotInitialized` 401 |
| Validation | `NegativeNotAllowed` 410, `InvalidArgument` 411, `InsufficientBalance` 412, `UnderflowOverflow` 413, `ArithmeticError` 414, `DivisionByZero` 415, `InvalidSharesMinted` 416, `OnlyPositiveAmountAllowed` 417, `NotAuthorized` 418 |
| Protocol | `ProtocolAddressNotFound` 420, `DeadlineExpired` 421, `ExternalError` 422, `SoroswapPairError` 423 |
| Blend-specific | `AmountBelowMinDust` 451, `UnderlyingAmountBelowMin` 452, `BTokensAmountBelowMin` 453, `InternalSwapError` 454, `SupplyNotFound` 455 |

The 45x block is Blend-specific but lives in the shared enum, so external strategies inherit codes they will never raise.

## Events

Every publisher takes the strategy name as its first topic element, so topics are `(strategy_name: String, symbol)` rather than a fixed contract prefix (`strategies/core/src/event.rs:25`).

| Topic symbol | Publisher | Payload |
|---|---|---|
| `deposit` | `emit_deposit` (`strategies/core/src/event.rs:22`) | `DepositEvent { amount, from }` (`strategies/core/src/event.rs:9`) |
| `harvest` | `emit_harvest` (`strategies/core/src/event.rs:47`) | `HarvestEvent { amount, from, price_per_share }` (`strategies/core/src/event.rs:32`) |
| `withdraw` | `emit_withdraw` (`strategies/core/src/event.rs:70`) | `WithdrawEvent { amount, from }` (`strategies/core/src/event.rs:57`) |

`price_per_share` is documented as an APY hint, optionally scaled by `SCALAR_12` (`strategies/core/src/event.rs:46`). The Blend strategy passes `shares_to_underlying(SCALAR_12, reserves)` (`strategies/blend/src/lib.rs:251`); the hodl and fixed_apr strategies pass `0`.

## Dependencies

`soroban-sdk` only (`strategies/core/Cargo.toml:15`). No `common`, no external protocol clients.

Consumed by: `vault` (`vault/Cargo.toml`), `factory` (declared, unused), every strategy crate in `strategies/*`, and any external repository that adds `defindex-strategy-core` from crates.io.

## Gotchas & invariants

- **The return-value contract is the whole integration risk.** `deposit` and `withdraw` must return the caller's *balance*, not the delta. `vault/src/strategies.rs:172` computes `gains_or_losses = returned - prev_balance - amount`; a strategy returning the deposited amount instead would book a fake loss every deposit.
- **`balance` must be in underlying units.** The vault sums strategy balances directly into `total_managed_funds` (`vault/src/funds.rs:91`) with no conversion.
- **`init_args` is positional and untyped.** There is no schema and no length check in the trait. Each strategy documents its own order; getting it wrong produces a runtime `into_val` panic at deploy time, not a compile error.
- **Bumping the crate version is a publish event.** `publish = true` and the version is pinned at `0.2.0` independently of the workspace `0.1.0` (`Cargo.toml:9`). Internal consumers use the path dependency declared at `Cargo.toml:19`, so a version bump alone does not update them.
- **`strategies/core/README.md` is out of date on two points:** its example `harvest` signature omits the `data: Option<Bytes>` parameter that the trait requires (`strategies/core/src/lib.rs:72`), and its `event::emit_deposit` example passes `String::from("...")` where the function takes a `soroban_sdk::String` built with `String::from_str(&e, ...)` (`strategies/core/src/event.rs:22`). Fix the README before pointing an external author at it.

## Testing

No `#[test]` cases in this crate. It is exercised indirectly through the strategy crates and `integration-test/`. There is no `Makefile` here; `strategies/Makefile:1` explicitly excludes `core` from the per-strategy build loop because it is a library, not a contract.
