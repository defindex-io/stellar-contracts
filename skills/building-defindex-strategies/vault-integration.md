# How the DeFindex vault uses your strategy

Reference for `building-defindex-strategies`. Paths are in
[defindex-io/stellar-contracts](https://github.com/defindex-io/stellar-contracts), verified
2026-10-13. If this file and the code disagree, the code wins.

## Call map

The vault talks to strategies only through `DeFindexStrategyClient` (`vault/src/strategies.rs:14`).

| Strategy call | Called from | `from` / `to` | What the vault does with the result |
|---|---|---|---|
| `asset()` | Vault constructor, once per strategy (`vault/src/lib.rs:161`) | none | Must equal the listed asset, otherwise `StrategyDoesNotSupportAsset` (102). Never checked again. |
| `balance(vault)` | Every user deposit and withdraw, `get_asset_amounts_per_shares`, `fetch_total_managed_funds`, `report`, `rescue`, rebalance `Unwind` (`vault/src/funds.rs:47`, loop at `:89`) | `from` = vault | Counted into total managed funds after subtracting `locked_fee`. Called for **paused strategies too**. |
| `deposit(amount, vault)` | `deposit(invest=true)` and rebalance `Invest` (`vault/src/strategies.rs:168`) | `from` = vault | Stored as `prev_balance`. `gains_or_losses = returned - prev_balance - amount` (`:172`). |
| `withdraw(amount, vault, to)` | User withdraw (`to` = **the user**), rebalance `Unwind`, fee distribution and `rescue` (`to` = vault) | `from` = vault | User withdraw: becomes `prev_balance` (`vault/src/lib.rs:396`). Unwind and rescue: ignored. |
| `harvest` | **Never called by the vault.** Your keeper calls it. | | |

## Deposit: one exact pull

Before calling `strategy.deposit`, the vault pre-authorizes **exactly one** sub-invocation:
`asset.transfer(vault, strategy, amount)` (`vault/src/strategies.rs:151-166`). Your
`deposit` must make that call with those arguments: `from = vault`, `to = your contract`,
the full `amount`. If you pull a different amount, send it somewhere else, or call
`transfer_from`, the vault's auth fails and the vault operation reverts.

If your protocol only accepts part of `amount` (Blend rounds to bTokens), keep the rest
as idle in your NAV. Do not refund it to the vault. Blend refunds it, and the vault records
the refund as a small loss on every deposit.

The vault never sends zero or a negative amount (guards at `vault/src/investment.rs:116`,
`vault/src/lib.rs:389`), but it does send **dust**: pro-rata remainders and small fee
amounts. If you reject tiny deposits, you revert someone's vault deposit or rebalance.

Auto-invest on user deposit is split pro-rata to current strategy balances, and **skipped
entirely while nothing is invested yet** (`vault/src/investment.rs:95`). A fresh vault only
reaches your strategy through a manager's `rebalance` `Invest`.

## Withdraw: the vault does not check what arrived

User withdraw (`vault/src/lib.rs:350-405`):

1. The vault burns the user's vault shares and computes the amount owed. `min_amounts_out`
   is checked against that amount **before** any strategy is touched.
2. Idle funds are paid first.
3. The remainder is split across strategies in listed order. Every strategy but the last
   gets `floor(remaining * its_balance / invested)`. **The last strategy gets the exact
   remainder** (`:376-387`), so it absorbs the rounding dust of the others: up to N-1 base
   units more than its pro-rata share.
4. Your `withdraw` is called with `to = the user`. The vault records the **requested**
   amount as paid. There is no balance check afterwards.

Consequences for you:
- Deliver exactly `amount` to `to`, or return an error. Under-delivering silently shorts
  the user, and the vault's books will say they were paid.
- Return the remaining balance. The vault stores it as `prev_balance`; a wrong value
  becomes a phantom gain (fees charged on nothing) or a phantom loss at the next report.
- `rescue` withdraws exactly what `balance(vault)` returned (`vault/src/lib.rs:459-467`).
  `withdraw(balance(vault))` must always succeed.
- Code reading suggests that if the last strategy is empty while others are not, a
  partial user withdraw can ask it for 1 to N-1 units and the user's withdraw reverts. This
  has not been reproduced in a test. Do not "fix" it by paying out other depositors'
  funds; reject the over-request with `InsufficientBalance`.

`withdraw` failures surface as `StrategyWithdrawError` (142): the vault uses `try_withdraw`
and maps any error (`vault/src/strategies.rs:136-138`).

## Reports, gains and fees

Per strategy the vault keeps `Report { prev_balance, gains_or_losses, locked_fee }`.

- **Recording.** `report(current)` adds `current - prev_balance` to `gains_or_losses`
  (`vault/src/report.rs:115`). When `prev_balance` is 0, the current balance becomes the
  baseline with no gain. It runs on user deposit and withdraw, `get_asset_amounts_per_shares`
  and the public `report()`. It does not run on a timer.
- **Locking.** `lock_fee` takes `vault_fee` bps of positive gains into `locked_fee` and
  resets gains to 0 (`vault/src/report.rs:51`). The Manager's `lock_fees` only locks gains
  already recorded (`vault/src/lib.rs:972-1002`); it does not read `balance()`.
- **Losses** stay recorded as negative gains and must be earned back before new fees accrue.
  Locked fees are **not** reduced by a loss. Only the Manager's `release_fees` reduces them
  (`vault/src/lib.rs:1014`).
- **Distribution** withdraws `locked_fee` from your strategy to the vault and pays the fee
  receivers (`vault/src/report.rs:189`), so fee time is a withdraw call too.
- **Deposits overwrite.** `invest_in_strategy` assigns `gains_or_losses` with `=` instead of
  adding (`vault/src/strategies.rs:172`). Gains recorded but not yet locked are lost from
  fees when a manager invests.

### The loss trap: `WrongLockedFees`

If `balance(vault) < locked_fee`, `fetch_strategy_invested_funds` returns
`WrongLockedFees` (113) (`vault/src/funds.rs:49-61`). That blocks deposit, withdraw,
`rescue` and `Unwind` for the **whole vault** until the Manager calls `release_fees`.
A strategy that can lose principal (bad debt, depeg, slippage on exit) can trigger this.
Keep losses small and gradual, and tell the vault operator how losses can happen.

## When your call fails

There is no error tolerance. Every failure reverts the vault operation.

| If this reverts | Blocked |
|---|---|
| `balance()` | Deposit and withdraw for **every asset in the vault**, `rescue`, `report`, `Unwind`. Pausing does not help: paused strategies are still queried. Strategies cannot be removed. Most vaults are not upgradable (`upgrade` needs `upgradable = true` at construction, `vault/src/lib.rs:822`). |
| `withdraw()` | User withdraws that need this strategy, `Unwind`, fee distribution, `rescue`. |
| `deposit()` | `deposit(invest=true)` when allocated to you, `Invest`. Pausing removes you from auto-invest. |

A strategy whose `balance()` can revert can permanently freeze every vault that lists it.

## Share price and yield timing

The vault prices its own shares off `idle + sum(balance - locked_fee)` with no smoothing.
A step change in your `balance()` (a harvest that re-supplies a week of rewards at once)
moves the vault share price in one block. Someone who deposits just before and withdraws
just after captures part of it. Release harvested profit linearly over a window at least
as long as the harvest interval, or harvest often in small amounts.

Value-accruing tokens whose balance never grows (RWA notes priced off-chain, wrapped
tokens with a rising exchange rate you do not realize) produce no balance delta, so the
vault sees no gain and charges no fee. Report yield as growth in the underlying asset.

## Vault facts that surprise strategy authors

- Vault shares have 7 decimals regardless of the asset (`vault/src/lib.rs:168`). Your
  `balance()` is in the asset's base units; the vault does no decimal conversion.
- The first vault deposit locks `MINIMUM_LIQUIDITY = 1000` vault shares to the vault itself
  (`vault/src/lib.rs:51`, `vault/src/deposit.rs:98-105`), so a small residual position
  stays invested in your strategy after every user exits.
- `rescue` withdraws everything, zeroes the report and pauses the strategy
  (`vault/src/lib.rs:431-483`). Paused strategies still count in managed funds and are
  still unwound pro-rata on user withdraws.
- Vault error codes 100-202 and strategy error codes 401-455 overlap with factory codes
  401-406. Read errors by emitting contract.
