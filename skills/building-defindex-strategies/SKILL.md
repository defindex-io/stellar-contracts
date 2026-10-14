---
name: building-defindex-strategies
description: Use when writing, reviewing or testing a Soroban strategy contract for DeFindex vaults on Stellar - implementing DeFindexStrategyTrait from the defindex-strategy-core crate, deciding what deposit, withdraw or balance return, handling shares, rounding, harvest, keepers or depositor allowlists, or debugging a vault that reverts, shows phantom gains or losses, WrongLockedFees, or StrategyWithdrawError.
---

# Building DeFindex strategies

## Overview

A DeFindex vault holds user funds and moves them into strategies. A strategy is a Soroban
contract implementing `DeFindexStrategyTrait` from the crates.io crate
`defindex-strategy-core`. The vault calls only the trait methods, trusts every number you
return, and **cannot remove you**. A strategy bug is a bug in every vault that lists it.

**Core principle:** `balance()` never fails, `deposit`/`withdraw` move exactly the amount asked
and return the *remaining position*, and every rounding favours the strategy.

Start from `example/`, a compiling reference strategy with tests against the real vault
WASM. Replace `example/src/upstream.rs` with your protocol and keep the rest.

## The trait (defindex-strategy-core 0.3.0)

| Method | Caller | Must do | Returns |
|---|---|---|---|
| `__constructor(asset, init_args: Vec<Val>)` | Deployer | Parse your own positional args. Only place a panic is acceptable. | |
| `asset()` | Vault, once at vault construction | Return the fixed underlying asset. | `Address` |
| `deposit(amount, from)` | Vault | `from.require_auth()`, allowlist check, pull exactly `transfer(from, self, amount)` once, mint shares | `from`'s balance **after**, in underlying units |
| `withdraw(amount, from, to)` | Vault | `from.require_auth()`, send exactly `amount` to `to` (the end user on user withdraws), burn shares. Never gated or paused. | `from`'s **remaining** balance, in underlying units |
| `balance(from)` | Vault, on every deposit and withdraw | Pure read, no auth, **never panics** | Underlying value of `from`'s shares, rounded down |
| `harvest(from, data: Option<Bytes>)` | Your keeper; the vault never calls it | Keeper-only, no-op when there is nothing to claim | `()` |

Use `defindex-strategy-core = "0.3.0"` with `soroban-sdk = "25"`. Emit its
`event::emit_deposit`, `emit_withdraw` and `emit_harvest` with your strategy name. 0.3.0
events use `#[contractevent]`: topics are `("deposit_event" | "withdraw_event" |
"harvest_event", strategy_name)`. The 0.2.0 source in `strategies/core/` of this repo emits
the older `(strategy_name, "deposit")` shape. Pass `price_per_share` scaled by 1e12 in the
harvest event. The vault itself reads only `balance()`, never your events.

## Rules that keep vaults alive

1. **`balance()` must not revert.** The vault calls it for every strategy, paused or not, on
   every user deposit and withdraw. A revert freezes the whole vault, every asset. Wrap
   external reads in `try_` calls and degrade to a conservative value. Don't use a spot price
   someone can move inside one transaction. Don't write storage in it.
2. **Return the post-operation position, never the delta.** The vault books
   `returned - previous - amount` as gain or loss and charges fees on it.
3. **Pull exactly once, exactly `amount`.** The vault pre-authorizes only
   `asset.transfer(vault, strategy, amount)`. Keep any amount your protocol can't take as
   idle inside your NAV. Don't refund it.
4. **Deliver exactly `amount` to `to`, or return an error.** The vault never checks what
   arrived, and it records the requested amount as paid.
5. **`withdraw(balance(vault))` always succeeds.** `rescue` does exactly that.
6. **Accept dust.** Expect 1-unit deposits and small withdraws from pro-rata remainders and fees.
7. **Deposits fail closed, withdrawals never do.** Allowlist depositors by address or vault
   WASM hash; otherwise anyone can deposit directly and skip the vault's fees. Never gate,
   pause or allowlist `withdraw`.
8. **Shares: virtual offset, conservative rounding.** `VIRTUAL_SHARES = 10^6` (OpenZeppelin
   ERC-4626 style), not a burned-first-deposit. Floor on mint, ceil on burn, floor on
   `balance`. Mint on the measured NAV delta, not on `amount`. Use the `env` variant of
   `soroban-fixed-point-math` (>= 1.5.0) so `shares * nav` widens to I256 instead of failing.
9. **No caller-chosen recipient or price floor anywhere.** The only payee is the vault's
   `to` on `withdraw`. Harvest `min_out` bounds a swap; it must never redirect funds.
10. **Yield is growth of the underlying.** Holding a token whose balance never grows shows
    zero gain to the vault. Smooth harvest profit (vesting) so a deposit-then-withdraw around
    a harvest can't capture it.
11. **Losses block vaults.** If `balance()` falls below the vault's locked fee, the vault
    returns `WrongLockedFees` on deposit and withdraw until its manager releases fees.
    Document every way your strategy can lose principal.

12. **Keep storage alive.** `balance()` must not write, so it cannot extend TTLs. Extend
    the instance and position TTLs on deposit, withdraw and harvest, and keep a keeper
    job bumping them. An archived config or position has to be restored before
    `balance()` can read it, so every vault call fails until someone restores it.

Details, with source lines for each claim: `vault-integration.md`.

## Protocols with fees, minimums or locks

- **Exit fees and redemption rounding:** report `balance()` net of what a full exit would
  cost, so `withdraw(balance(vault))` can always deliver. Redeem enough gross to deliver
  exactly `amount`, then re-measure NAV after the redemption instead of assuming it fell by
  `amount`. A supply that costs a fee shows the vault a small loss on each deposit. Tell
  the vault operator.
- **Minimum supply sizes:** count idle underlying in NAV at face value. Supply once idle
  crosses the minimum, or from a keeper sweep. On withdraw, pay from idle first. If a
  redemption would leave the position below the minimum, redeem it all and keep the rest
  idle. Never reject a vault amount because it is below the protocol minimum.
- **Upstream paused or illiquid:** `balance()` still returns a value. `withdraw` pays what
  idle can cover in full, or returns an error (`InsufficientBalance` or `ExternalError`). It
  never pays a partial amount. Keep an idle buffer if the protocol pauses often.

## Testing

- `cargo test` in `example/` runs the invariant tests. `cargo test --features vault-it`
  drives the **real vault WASM** (build it with `cd vault && make build` in this repo, or
  fetch the deployed one, see below). A fake vault address can't catch report bugs or a
  deposit that pulls the wrong transfer; the real vault can.
- `mock_all_auths()` proves nothing about auth. Test keeper, admin and the vault's
  pre-authorized pull with `mock_auths`, as `example/src/test_vault.rs` does: pulling
  `amount - 1` there fails with `Error(Auth, InvalidAction)`.
- Required cases: `deposit` and `withdraw` return values, full exit to 0, 1-unit deposit
  after a large one, over-withdraw rejected, non-allowlisted deposit rejected, `withdraw`
  after de-listing, inflation attack, 18-decimal amounts, `balance()` with every upstream
  dependency failing, non-keeper harvest rejected.
- For production, allowlist the **deployed** vault WASM hash. Read it from chain
  (`stellar contract fetch --id <vault> -o vault.wasm`, then `sha256sum vault.wasm`). A
  local build may differ.

Before handing a strategy to DeFindex, run through `review-checklist.md`.

## Common mistakes

| Mistake | What breaks |
|---|---|
| `deposit` returns `amount` or minted shares | Phantom gain/loss each deposit, fees charged on nothing |
| `balance()` calls an oracle or pool with a plain client call | One dead feed freezes every vault listing you |
| `balance()` returns shares, not underlying | Vault TVL and share price wrong by the share ratio |
| Refunding unused deposit to the vault | Small loss recorded on every deposit |
| Withdraw sends less than `amount` on rounding | User shorted while vault books show full payment |
| Rejecting amounts below a protocol minimum | Vault deposits and rebalances revert on dust |
| No depositor allowlist | Direct deposits skip vault fees; observed on mainnet with an early strategy |
| Withdraw behind pause or allowlist | Vault can't exit; funds stuck |
| Open `harvest`, or `min_out = 0` default | Anyone triggers sandwichable reward swaps |
| Large step-jump harvests | Yield sniped by deposit-harvest-withdraw |
| Plain `FixedPoint` mul-div with 18-decimal asset | `balance()` overflow, vault frozen |
| Copying `strategies/soroswap` or `strategies/xycloans` | Prototypes: they return 0 from `deposit`, and `asset()` returns the router |
