# Vault Module

> **Living document.** Read this before modifying the module. Update it in the same change whenever the module's behavior, endpoints, files, or dependencies change.

**Source:** `vault/` · **Last verified:** 2026-09-05

Background design notes live in [`../defindex-contracts.md`](../defindex-contracts.md) sections 3, 7 and 11. That file is the long-form audit reference. This file is the working map. See "Known doc drift" below.

## Purpose

The DeFindex Vault is the core protocol contract. It is a SEP-41 share token that custodies one or more underlying assets, routes them into `DeFindexStrategyTrait` strategies, tracks per-strategy profit and loss, and splits performance fees between the vault fee receiver and the DeFindex protocol receiver. Every other module in this repo exists to deploy it, feed it, or be called by it. Changing share math, report math, or the role model changes on-chain behavior for every deployed vault and for every downstream SDK or indexer that reads its events.

## Structure

| File | Purpose |
|---|---|
| `vault/src/lib.rs` | The contract. All three trait impls and the entrypoint bodies. |
| `vault/src/interface.rs` | `VaultTrait`, `AdminInterfaceTrait`, `VaultManagementTrait` declarations and doc comments. |
| `vault/src/access.rs` | `RolesDataKey`, `AccessControl`, the whole authorization model. |
| `vault/src/storage.rs` | `DataKey`, instance and persistent accessors, TTL bumps, `Report` storage. |
| `vault/src/report.rs` | `Report` struct plus `lock_fee` / `release_fee` / `report` / `distribute_strategy_fees`. |
| `vault/src/funds.rs` | Idle, invested and total-managed-funds reads. |
| `vault/src/deposit.rs` | `process_deposit`, share minting, `MINIMUM_LIQUIDITY` handling. |
| `vault/src/investment.rs` | `generate_investment_allocations` (proportional auto-invest). |
| `vault/src/strategies.rs` | Strategy client helpers, invest, unwind, pause, unpause. |
| `vault/src/router.rs` | Soroswap swap helpers used by `rebalance`. |
| `vault/src/utils.rs` | Multi-asset optimal-amount and share math, asset/strategy validation. |
| `vault/src/models.rs` | `Instruction`, `StrategyAllocation`, `CurrentAssetInvestmentAllocation`. |
| `vault/src/events.rs` | Every emitted event struct and publisher. |
| `vault/src/error.rs` | `ContractError`. |
| `vault/src/token/` | The SEP-41 share token (balance, allowance, metadata, total supply). |
| `vault/src/test.rs`, `vault/src/test/` | Unit tests. Compile against built WASM, see Testing. |

## Public surface

Contract type `DeFindexVault` (`vault/src/lib.rs:56`).

### VaultTrait (`vault/src/lib.rs:59`)

| Function | Auth | Notes |
|---|---|---|
| `__constructor(e, assets: Vec<AssetStrategySet>, roles: Map<u32, Address>, vault_fee: u32, defindex_protocol_receiver: Address, defindex_protocol_rate: u32, soroswap_router: Address, name_symbol: Map<String, String>, upgradable: bool)` (`vault/src/lib.rs:115`) | none, constructor | Panics `RolesIncomplete` if any of the 4 roles is missing (`vault/src/lib.rs:128`), `MetadataIncomplete` without `name`/`symbol` (`vault/src/lib.rs:134`), `MaximumFeeExceeded` if `defindex_protocol_rate > 9000` (`vault/src/lib.rs:142`), `StrategyDoesNotSupportAsset` if any strategy's `asset()` differs (`vault/src/lib.rs:161`). Token decimals are hardcoded to 7 (`vault/src/lib.rs:168`). |
| `deposit(e, amounts_desired: Vec<i128>, amounts_min: Vec<i128>, from: Address, invest: bool) -> Result<(Vec<i128>, i128, Option<Vec<Option<AssetInvestmentAllocation>>>), ContractError>` (`vault/src/lib.rs:231`) | `from.require_auth()` (`vault/src/lib.rs:239`) | Depositor only. Calls `fetch_total_managed_funds(&e, true)`, so it locks fees as a side effect (`vault/src/lib.rs:243`). |
| `withdraw(e, withdraw_shares: i128, min_amounts_out: Vec<i128>, from: Address) -> Result<Vec<i128>, ContractError>` (`vault/src/lib.rs:299`) | `from.require_auth()` (`vault/src/lib.rs:301`) | Burns first, then pays out (`vault/src/lib.rs:330`). Also locks fees (`vault/src/lib.rs:308`). |
| `rescue(e, strategy_address: Address, caller: Address) -> Result<(), ContractError>` (`vault/src/lib.rs:431`) | `require_any_role([EmergencyManager, Manager], caller)` (`vault/src/lib.rs:440`) | Distributes pending fees, unwinds the whole strategy balance, zeroes the report, pauses the strategy. |
| `pause_strategy(e, strategy_address: Address, caller: Address)` (`vault/src/lib.rs:497`) | `require_any_role([EmergencyManager, Manager], caller)` (`vault/src/lib.rs:505`) | Flips `Strategy.paused` inside the stored `AssetStrategySet`. |
| `unpause_strategy(e, strategy_address: Address, caller: Address)` (`vault/src/lib.rs:527`) | `require_any_role([EmergencyManager, Manager], caller)` (`vault/src/lib.rs:535`) | |
| `get_assets(e) -> Result<Vec<AssetStrategySet>, ContractError>` (`vault/src/lib.rs:552`) | none | |
| `fetch_total_managed_funds(e) -> Result<Vec<CurrentAssetInvestmentAllocation>, ContractError>` (`vault/src/lib.rs:567`) | none | Calls the internal helper with `lock_fees = false` (`vault/src/lib.rs:569`). |
| `get_asset_amounts_per_shares(e, vault_shares: i128) -> Result<Vec<i128>, ContractError>` (`vault/src/lib.rs:586`) | none | Calls the internal helper with `lock_fees = true` (`vault/src/lib.rs:594`), so this read mutates reports. |
| `get_fees(e) -> (u32, u32)` (`vault/src/lib.rs:614`) | none | Returns `(vault_fee, defindex_protocol_fee)`. |
| `report(e) -> Result<Vec<Report>, ContractError>` (`vault/src/lib.rs:656`) | none | Refreshes every strategy report from its live balance. Does not lock fees. |

### AdminInterfaceTrait (`vault/src/lib.rs:683`)

| Function | Auth | Notes |
|---|---|---|
| `set_fee_receiver(e, caller: Address, new_fee_receiver: Address)` (`vault/src/lib.rs:695`) | `require_any_role([Manager, VaultFeeReceiver], caller)` (`vault/src/access.rs:74`) | |
| `get_fee_receiver(e) -> Result<Address, ContractError>` (`vault/src/lib.rs:710`) | none | |
| `set_manager(e, new_manager: Address) -> Result<(), ContractError>` (`vault/src/lib.rs:726`) | `require_role(Manager)` (`vault/src/access.rs:87`) | No two-step handoff. The `ManagerNotInQueue` / `SetManagerBeforeTime` / `QueueEmpty` errors exist but no queue is implemented. |
| `get_manager(e) -> Result<Address, ContractError>` (`vault/src/lib.rs:742`) | none | |
| `set_emergency_manager(e, emergency_manager: Address)` (`vault/src/lib.rs:758`) | `require_role(Manager)` (`vault/src/access.rs:96`) | Manager only, despite the interface doc comment saying manager or emergency manager (`vault/src/interface.rs:348`). |
| `get_emergency_manager(e) -> Result<Address, ContractError>` (`vault/src/lib.rs:773`) | none | |
| `set_rebalance_manager(e, new_rebalance_manager: Address)` (`vault/src/lib.rs:789`) | `require_role(Manager)` (`vault/src/access.rs:105`) | |
| `get_rebalance_manager(e) -> Result<Address, ContractError>` (`vault/src/lib.rs:804`) | none | |
| `upgrade(e, new_wasm_hash: BytesN<32>) -> Result<(), ContractError>` (`vault/src/lib.rs:822`) | `require_role(Manager)` (`vault/src/lib.rs:828`) | Returns `NotUpgradable` when the `Upgradable` flag is false (`vault/src/lib.rs:823`). The flag is set once at construction and there is no setter. |

### VaultManagementTrait (`vault/src/lib.rs:836`)

| Function | Auth | Notes |
|---|---|---|
| `rebalance(e, caller: Address, instructions: Vec<Instruction>) -> Result<(), ContractError>` (`vault/src/lib.rs:846`) | `require_any_role([RebalanceManager, Manager], caller)` (`vault/src/lib.rs:850`) | Panics `NoInstructions` on an empty vector (`vault/src/lib.rs:856`). |
| `lock_fees(e, new_fee_bps: Option<u32>) -> Result<Vec<Report>, ContractError>` (`vault/src/lib.rs:972`) | `require_role(Manager)` (`vault/src/lib.rs:976`) | Passing `Some(bps)` rewrites the vault fee before locking (`vault/src/lib.rs:979`). |
| `release_fees(e, strategy: Address, amount: i128) -> Result<Report, ContractError>` (`vault/src/lib.rs:1014`) | `require_role(Manager)` (`vault/src/lib.rs:1018`) | Moves locked fee back into `gains_or_losses`, giving value back to depositors. |
| `distribute_fees(e, caller: Address) -> Result<Vec<(Address, i128)>, ContractError>` (`vault/src/lib.rs:1039`) | `require_any_role([Manager, VaultFeeReceiver], caller)` (`vault/src/lib.rs:1043`) | |

### Share token (`vault/src/token/contract.rs`)

`VaultToken` implements `soroban_sdk::token::Interface` (`vault/src/token/contract.rs:58`): `allowance`, `approve` (`from.require_auth()`, line 67), `balance`, `transfer` (`from.require_auth()`, line 89), `transfer_from` (`spender.require_auth()`, line 103), `burn` (`from.require_auth()`, line 118), `burn_from` (`spender.require_auth()`, line 123), `decimals`, `name`, `symbol`. `total_supply` is an extra method (`vault/src/token/contract.rs:52`). There is no public `mint`; supply only changes through `deposit` and `withdraw` via `internal_mint` / `internal_burn` (`vault/src/token/contract.rs:19`, `:32`).

## Key methods

- **`process_deposit`** (`vault/src/deposit.rs:11`): the single-asset path and the multi-asset path diverge. With one asset, shares are `total_supply * amount / total_amount` (`vault/src/deposit.rs:73`). With several assets and a non-zero supply, `calculate_deposit_amounts_and_shares_to_mint` tries each asset in turn as the enforced ratio anchor and returns the first that satisfies every other asset's desired and min bounds (`vault/src/utils.rs:186`). If none does, it returns `NoOptimalAmounts` (`vault/src/utils.rs:241`). On the very first multi-asset deposit shares are simply the sum of the desired amounts (`vault/src/deposit.rs:34`).
- **`mint_shares`** (`vault/src/deposit.rs:88`): on the first deposit, `MINIMUM_LIQUIDITY = 1000` (`vault/src/lib.rs:51`) shares are minted to the vault itself and subtracted from the depositor. This is the anti-inflation guard. It also means the first depositor must mint strictly more than 1000 shares or the call panics with `InsufficientAmount` (`vault/src/deposit.rs:98`).
- **`Report::report(current_balance)`** (`vault/src/report.rs:115`): accumulates `current_balance - prev_balance` into `gains_or_losses`. A `prev_balance` of 0 is treated as "first report", so the current balance becomes the baseline and no gain is booked (`vault/src/report.rs:117`). That special case is why `rescue` writes a fully zeroed report rather than deleting the key (`vault/src/lib.rs:470`).
- **`Report::lock_fee(fee_rate)`** (`vault/src/report.rs:51`): returns early when `gains_or_losses <= 0` (`vault/src/report.rs:52`), so losses stay booked in `gains_or_losses` and must be earned back before any new fee accrues. Fee is `gains * fee_rate / SCALAR_BPS` where `SCALAR_BPS = 10_000` (`vault/src/constants.rs:2`); the gain counter is then reset to 0.
- **`distribute_strategy_fees`** (`vault/src/report.rs:189`): unwinds `locked_fee` worth of underlying out of the strategy, pays `locked_fee * defindex_rate / 10_000` to the protocol receiver and the remainder to the vault fee receiver (`vault/src/report.rs:200`-`:215`), then sets `prev_balance` to the strategy's post-unwind balance and clears `locked_fee`. The protocol rate is a share of the vault fee, not an extra charge on top of it.
- **`fetch_strategy_invested_funds(e, strategy, report_and_lock_fees)`** (`vault/src/funds.rs:45`): always subtracts `locked_fee` from the raw strategy balance, and returns `WrongLockedFees` if the locked amount somehow exceeds the balance (`vault/src/funds.rs:51`). The boolean only decides whether new fees are locked first.
- **`invest_in_strategy`** (`vault/src/strategies.rs:142`): pre-authorizes exactly one sub-invocation, the underlying token `transfer` from the vault to the strategy (`vault/src/strategies.rs:151`), then calls `strategy.deposit`. It recomputes `gains_or_losses` as `returned_balance - prev_balance - amount` (`vault/src/strategies.rs:172`), which is how a strategy that mis-reports its balance would corrupt vault accounting.
- **`unwind_from_strategy`** (`vault/src/strategies.rs:128`): uses `try_withdraw` and collapses every failure into `StrategyWithdrawError` (`vault/src/strategies.rs:138`), so the underlying `StrategyError` is not surfaced to callers.
- **`generate_investment_allocations`** (`vault/src/investment.rs:82`): splits a deposit across strategies in proportion to their current invested amounts, giving the remainder to the last strategy to avoid rounding dust (`vault/src/investment.rs:101`). Assets with zero prior investment are skipped entirely (`vault/src/investment.rs:95`), so `deposit(..., invest = true)` leaves funds idle on a brand-new vault. Paused strategies get `None` (`vault/src/investment.rs:116`).
- **`internal_swap_exact_tokens_for_tokens` / `internal_swap_tokens_for_exact_tokens`** (`vault/src/router.rs:22`, `:78`): both refuse tokens that are not vault assets (`vault/src/router.rs:31`, `:87`), resolve the pair through the router's `router_pair_for`, pre-authorize a single transfer to that pair, then invoke the router. The exact-out path prices the swap locally with `soroswap_library::get_amount_in` and rejects `amount_in > amount_in_max` with `ExcessiveInputAmount` (`vault/src/router.rs:107`).

## Storage & TTL

`DataKey` (`vault/src/storage.rs:22`), all **instance** storage except `Report`:

`TotalAssets`, `AssetStrategySet(u32)`, `DeFindexProtocolFeeReceiver`, `Upgradable`, `VaultFee`, `SoroswapRouter`, `DeFindexProtocolFeeRate`, `Factory`, `Report(Address)`.

- `Factory` is declared but never read or written anywhere in `vault/src` (verified by grep). Dead key.
- `Report(Address)` is **persistent** (`vault/src/storage.rs:122`). Both `set_report` and `get_report` bump its TTL (`vault/src/storage.rs:127`, `:137`). A missing report reads back as all-zero rather than erroring (`vault/src/storage.rs:142`).
- Roles live in instance storage under a separate `RolesDataKey` enum (`vault/src/access.rs:6`, written at `vault/src/access.rs:36`).

TTL constants (`vault/src/storage.rs:6`-`:12`): `DAY_IN_LEDGERS = 17280`, instance bump 30 days with a 29-day threshold, persistent bump 120 days with a 100-day threshold. `extend_instance_ttl` (`vault/src/storage.rs:14`) is called at the top of essentially every entrypoint.

The share token keeps its own key space and TTLs (`vault/src/token/storage_types.rs:25`): `Allowance(AllowanceDataKey)`, `Balance(Address)`, `TotalSupply`. Balance bump is 120 days with a 119-day threshold (`vault/src/token/storage_types.rs:7`).

## Errors

`ContractError` (`vault/src/error.rs:7`), `#[repr(u32)]`:

- Initialization 100-108: `NotInitialized` 100, `InvalidRatio` 101, `StrategyDoesNotSupportAsset` 102, `NoAssetAllocation` 103, `RolesIncomplete` 104, `MetadataIncomplete` 105, `MaximumFeeExceeded` 106, `DuplicatedAsset` 107, `DuplicatedStrategy` 108.
- Validation 110-129: `AmountNotAllowed` 110, `InsufficientBalance` 111, `WrongAmountsLength` 112, `WrongLockedFees` 113, `InsufficientManagedFunds` 114, `MissingInstructionData` 115, `UnsupportedAsset` 116, `InsufficientAmount` 117, `NoOptimalAmounts` 118, `WrongInvestmentLength` 119, `WrongAssetAddress` 122, `WrongStrategiesLength` 123, `AmountOverTotalSupply` 124, `NoInstructions` 125, `NotUpgradable` 126, `UnwindMoreThanAvailable` 128, `InsufficientFeesToRelease` 129.
- Arithmetic: `ArithmeticError` 120, `Overflow` 121, `Underflow` 127.
- Authorization 130-134: `Unauthorized` 130, `RoleNotFound` 131, `ManagerNotInQueue` 132, `SetManagerBeforeTime` 133, `QueueEmpty` 134. The last three are unused.
- Strategy 140-144: `StrategyNotFound` 140, `StrategyPausedOrNotFound` 141, `StrategyWithdrawError` 142, `StrategyInvestError` 143, `StrategyPaused` 144.
- Asset 150-151, input 160-162, `LibrarySortIdenticalTokens` 190, `SoroswapRouterError` 200, `SwapExactInError` 201, `SwapExactOutError` 202.

`SoroswapLibraryError` is mapped in (`vault/src/error.rs:76`); every variant other than `SortIdenticalTokens` hits a bare `panic!` (`vault/src/error.rs:80`).

## Events

All events publish under the topic prefix `"DeFindexVault"` (`vault/src/events.rs`):

| Topic | Publisher | Payload struct |
|---|---|---|
| `deposit` | `vault/src/events.rs:35` | `VaultDepositEvent` (`:9`) |
| `withdraw` | `vault/src/events.rs:67` | `VaultWithdrawEvent` (`:41`) |
| `rescue` | `vault/src/events.rs:93` | `EmergencyWithdrawEvent` (`:73`) |
| `paused` | `vault/src/events.rs:112` | `StrategyPausedEvent` (`:99`) |
| `unpaused` | `vault/src/events.rs:131` | `StrategyUnpausedEvent` (`:118`) |
| `nreceiver` | `vault/src/events.rs:150` | `FeeReceiverChangedEvent` (`:137`) |
| `nmanager` | `vault/src/events.rs:165` | `ManagerChangedEvent` (`:156`) |
| `nemanager` | `vault/src/events.rs:182` | `EmergencyManagerChangedEvent` (`:171`) |
| `rbmanager` | `vault/src/events.rs:199` | `RebalanceManagerChangedEvent` (`:188`) |
| `dfees` | `vault/src/events.rs:214` | `FeesDistributedEvent` (`:205`) |
| `rebalance` | `vault/src/events.rs:240`, `:264`, `:287`, `:308`, `:333` | `UnwindEvent` (`:220`), `InvestEvent` (`:246`), `SwapExactInEvent` (`:272`), `SwapExactOutEvent` (`:293`), `ZapperEvent` (`:315`) |

All five rebalance variants share the single `rebalance` topic and carry a `rebalance_method` symbol inside the payload (`unwind`, `invest`, `SwapEIn`, `SwapEOut`, `Zapper`). Indexers must discriminate on the payload, not the topic. `ZapperEvent` has a publisher but no caller, since `Instruction::Zapper` is commented out (`vault/src/models.rs:56`).

## Dependencies

Crates (`vault/Cargo.toml`): `soroban-sdk`, `soroban-token-sdk`, `defindex-strategy-core` (path `strategies/core`), `common`, `soroswap-library` (crates.io `2.0.0`, pinned in the workspace at `Cargo.toml:18`).

Cross-contract calls made by the vault:

- **Strategy contracts** through `DeFindexStrategyClient` (`vault/src/strategies.rs:14`): `asset()` at construction (`vault/src/lib.rs:160`), `balance()` (`vault/src/funds.rs:47`, `vault/src/lib.rs:668`), `deposit()` (`vault/src/strategies.rs:168`), `try_withdraw()` (`vault/src/strategies.rs:136`).
- **Underlying asset tokens** through `soroban_sdk::token::TokenClient`: pull on deposit (`vault/src/deposit.rs:53`), pay out on withdraw (`vault/src/lib.rs:354`, `:362`), idle balance read (`vault/src/funds.rs:24`), fee payouts (`vault/src/report.rs:213`-`:215`).
- **Soroswap router**, address stored at construction (`vault/src/lib.rs:149`, key at `vault/src/storage.rs:28`): `router_pair_for`, `swap_exact_tokens_for_tokens`, `swap_tokens_for_exact_tokens` (`vault/src/router.rs:45`, `:70`, `:139`).

Called by: the factory, which deploys this WASM and passes the constructor args (`factory/src/lib.rs:210`-`:220`), and the TypeScript tooling in `src/utils/vault.ts`.

## Gotchas & invariants

- **Reads are not pure.** `get_asset_amounts_per_shares` and any path that calls `fetch_total_managed_funds(&e, true)` write reports and lock fees. Only `fetch_total_managed_funds()` (`vault/src/lib.rs:567`) uses `lock_fees = false`. Simulating the mutating reads on a live vault changes state if you submit.
- **`withdraw` burns before it pays.** `internal_burn` runs at `vault/src/lib.rs:330`, before any transfer. A failed unwind therefore reverts the whole transaction, which is the intent, but any future change that swallows an unwind error would burn shares for nothing.
- **Withdraw slippage is checked per asset against the pro-rata amount, not the delivered amount.** The `min_amounts_out` check compares `requested_withdrawal_amount` (`vault/src/lib.rs:347`), computed before unwinding. If a strategy returns less than requested, the shortfall lands in `withdrawn_amounts` but is not re-checked.
- **`deposit(invest = true)` does nothing on a fresh vault.** `generate_investment_allocations` skips any asset with `invested_amount == 0` (`vault/src/investment.rs:95`). A first deposit stays idle until the manager calls `rebalance` with `Instruction::Invest`.
- **Rebalance `Invest` distributes fees after investing** (`vault/src/lib.rs:894` then `:903`), while `Unwind` distributes before (`vault/src/lib.rs:869`). Do not assume a uniform ordering.
- **Fee rate can change mid-flight.** `lock_fees(Some(bps))` rewrites `VaultFee` before locking (`vault/src/lib.rs:979`), and the new rate applies to gains already accrued but not yet locked.
- **Losses are sticky.** `lock_fee` skips negative gains (`vault/src/report.rs:52`) but never reduces `locked_fee`; only `release_fees` does, and that is manager-driven.
- **First depositor must exceed `MINIMUM_LIQUIDITY`.** 1000 shares are permanently locked in the contract's own balance (`vault/src/deposit.rs:101`), so total supply never returns to zero after the first deposit.
- **Upgradability is decided once.** `set_is_upgradable` is only called from the constructor (`vault/src/lib.rs:147`); there is no post-deployment setter. `is_upgradable` defaults to `true` when the key is absent (`vault/src/storage.rs:177`).
- **Role setters are asymmetric.** `set_fee_receiver` accepts the current fee receiver as well as the manager (`vault/src/access.rs:74`), but the emergency and rebalance manager setters are manager-only (`vault/src/access.rs:96`, `:105`) even though `interface.rs` doc comments suggest otherwise.
- **`require_any_role` requires the caller to *be* the role holder**, not merely to be authorized by one (`vault/src/access.rs:52`). A mismatch panics with `Unauthorized` rather than returning an error.
- **Assets and strategies are fixed at construction.** There is no add/remove asset or strategy entrypoint; `pause_strategy` is the only lifecycle control. Changing the set means deploying a new vault.
- **`get_defindex_protocol_fee_rate`, `get_vault_fee`, `get_soroswap_router` all `.unwrap()`** (`vault/src/storage.rs:87`, `:118`, `:101`). They panic on an uninitialized vault instead of returning `NotInitialized`.

### Known doc drift in `../defindex-contracts.md`

- Line 5 and Appendix A state that all paths are relative to `apps/contracts/`. That prefix is from the old `paltalabs/defindex` monorepo. In this repo the same paths are relative to the repo root.
- Section 3.11 lists the storage layout correctly, including the note that `Factory` is unused. Verified accurate.
- Section 3.12 labels `report()` as read-only. It writes updated reports to storage (`vault/src/lib.rs:672`).

## Testing

`vault/src/test.rs` plus `vault/src/test/vault/` hold 102 `#[test]` cases across 14 files, covering initialize, deposit, withdraw, invest, rebalance, fees, rescue, router swaps, upgrade, admin and budget.

Run with `make -C vault test`, which builds first (`vault/Makefile:5`). The build step is mandatory: the tests `contractimport!` the optimized WASM of the vault itself and of the hodl, unsafe_hodl and fixed_apr strategies from `../target/wasm32v1-none/release/*.optimized.wasm` (`vault/src/test.rs:17`, `:30`, `:43`, `:59`). Soroswap factory, router and pair come from the vendored WASMs in `soroswap/` (`vault/src/test/soroswap_setup.rs:7`, `:13`, `:28`).
