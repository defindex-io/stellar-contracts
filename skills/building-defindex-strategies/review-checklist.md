# Strategy review checklist

Run this before handing a strategy to DeFindex, and when reviewing one. Every "no" is a
blocker unless the design doc explains why. Cite `file:line` in your code for each answer.

## Interface

- [ ] Depends on `defindex-strategy-core = "0.3.0"` and implements all six trait methods.
- [ ] `asset()` returns a value fixed at construction.
- [ ] `deposit` returns `from`'s position after the deposit, in underlying base units.
- [ ] `withdraw` returns `from`'s remaining position, in underlying base units.
- [ ] `balance` returns underlying units (not shares, not a rate), rounded down.
- [ ] Core `emit_deposit`, `emit_withdraw`, `emit_harvest` are emitted; harvest carries
      `price_per_share` scaled by 1e12.

## `balance()` cannot freeze a vault

- [ ] No `require_auth`, no storage writes, no events.
- [ ] Every external call is a `try_` call with a defined fallback, or a call to a contract
      that cannot fail (a Stellar Asset Contract `balance`).
- [ ] No spot price or pool reserve that can be moved inside one transaction.
- [ ] No overflow path: mul-div uses the `env` (I256-widening) variant, tested with
      18-decimal-sized amounts.
- [ ] Listed: every external contract `balance()` touches, and who keeps its TTL alive.

## Money movement

- [ ] `deposit` calls `asset.transfer(from, this, amount)` exactly once with the full amount.
- [ ] No refund of unused deposit to `from`.
- [ ] Shares minted on the measured NAV delta, priced on NAV before the deposit.
- [ ] `withdraw` delivers exactly `amount` to `to`, or returns an error; never less.
- [ ] `withdraw(balance(from))` succeeds (full exit, used by vault `rescue`).
- [ ] 1-unit deposit and 1-unit withdraw succeed on a large position.
- [ ] Storage effects happen before external transfers where possible.

## Access

- [ ] Deposit allowlist by address and/or vault WASM hash; ships empty (fails closed).
- [ ] `withdraw` has no allowlist, pause, or mode gate.
- [ ] `harvest` is keeper-only: `keeper.require_auth()` and `from == keeper`.
- [ ] No method sends tokens to a caller-supplied address other than `to` in `withdraw`.
- [ ] No method accepts a caller-supplied price floor that can release funds.
- [ ] Privileged setters can only tighten safety limits immediately; loosening is
      timelocked or impossible.
- [ ] Key rotation needs both the old and the new key to sign.

## Accounting and yield

- [ ] Virtual-offset shares (`VIRTUAL_SHARES = 10^6`); floor mint, ceil burn, floor balance.
- [ ] Inflation/donation attack test passes.
- [ ] Yield shows up as growth of the underlying in `balance()`.
- [ ] Harvest profit is vested or harvested in small increments; no large step jumps.
- [ ] Harvest with nothing to claim returns `Ok(())`.
- [ ] Harvest swaps require `min_out > 0`; malformed `data` returns an error.
- [ ] Every way principal can be lost is written down (bad debt, depeg, exit slippage,
      protocol pause), with how `balance()` behaves in each.

## Tests

- [ ] Integration tests against the real vault WASM: invest, report shows no phantom gain,
      user withdraw through the strategy, yield recorded as gain.
- [ ] Auth tests with `mock_auths`, not only `mock_all_auths`.
- [ ] `balance()` tested with each upstream dependency failing or archived.
- [ ] Release build has `overflow-checks = true`.

## Deployment

- [ ] Constructor `init_args` documented positionally, with a deploy script.
- [ ] Deployed vault WASM hash allowlisted (read from chain, not a local build).
- [ ] Upgradability decision recorded. A non-upgradable strategy with a bug can only be
      exited through the vault's `rescue`.
