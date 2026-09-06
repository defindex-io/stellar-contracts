# Module Documentation Index

Living docs, one per module. **Read the relevant doc before modifying a module; update it in the same change.** See "Module Documentation Convention" in [`../../CLAUDE.md`](../../CLAUDE.md) for the workflow.

Do not load every doc. Find the one row whose source path contains the code you are about to touch, and read only that file.

## Contracts

| Doc | Module | One-liner |
|---|---|---|
| [vault.md](vault.md) | `vault/` | The core vault contract: share token, deposits, withdrawals, rebalance, fee reports. |
| [factory.md](factory.md) | `factory/` | Deploys vaults from a pinned WASM hash and stamps them with the protocol fee. |
| [blend-strategy.md](blend-strategy.md) | `strategies/blend/` | The production strategy: supplies to a Blend V2 pool and auto-compounds BLND through Soroswap. |
| [reference-strategies.md](reference-strategies.md) | `strategies/hodl/`, `strategies/unsafe_hodl/`, `strategies/fixed_apr/`, `strategies/soroswap/`, `strategies/xycloans/` | Test fixtures and unfinished prototypes. Not for production. |

## Shared crates

| Doc | Module | One-liner |
|---|---|---|
| [strategy-core.md](strategy-core.md) | `strategies/core/` | The published `DeFindexStrategyTrait`, `StrategyError` and events that every strategy implements. |
| [common.md](common.md) | `common/` | `Strategy` and `AssetStrategySet` contract types shared by vault and factory, plus a string helper. |

## Tooling

| Doc | Module | One-liner |
|---|---|---|
| [deploy-tooling.md](deploy-tooling.md) | `src/`, `bash/`, `configs.json`, `public/` | TypeScript and bash deploy scripts, address books, network config, on-chain smoke tests. |
| [integration-test.md](integration-test.md) | `integration-test/` | Cross-contract Rust suite that runs factory, vault and strategies against real built WASM. |

## Related, not module docs

- [`../defindex-contracts.md`](../defindex-contracts.md): long-form protocol knowledge base for auditors and integrators. Its file paths still carry the old `apps/contracts/` monorepo prefix; in this repo they are relative to the repo root.
- `../audits/2025_03_18_ottersec_defindex_audit.pdf`: OtterSec audit.
- [`../../README.md`](../../README.md): build and deploy walkthrough. Several paths are stale, see [deploy-tooling.md](deploy-tooling.md).
