# Deploy Tooling Module

> **Living document.** Read this before modifying the module. Update it in the same change whenever the module's behavior, endpoints, files, or dependencies change.

**Source:** `src/`, `bash/`, `configs.json`, `public/`, `package.json` · **Last verified:** 2026-09-05

The prose walkthrough of the deploy sequence lives in [`../../README.md`](../../README.md) sections "Deployments" through "Full deployment sequence". Several of its paths are stale, see Gotchas.

## Purpose

TypeScript and bash scripts that install WASM, deploy the factory, strategies and vaults, keep address books, and drive on-chain smoke tests against testnet and mainnet. This is the only module that touches secrets and live networks. It is also the record of what is actually deployed: `public/mainnet.contracts.json` and `public/testnet.contracts.json` are checked in.

## Structure

| Path | Purpose |
|---|---|
| `src/deploy_factory.ts` | Installs the vault and factory WASM, deploys the factory. |
| `src/deploy_vault.ts` | Calls `create_defindex_vault` on the factory. |
| `src/strategies/deploy_blend.ts` | Deploys Blend strategies from `blend_deploy_config.json` and bootstraps each with a first deposit. |
| `src/strategies/multi_deploy_blend.ts` | Deploys N identical Blend strategies for tests. |
| `src/strategies/deploy_strategies.ts` | Generic strategy deploy driven by the `Strategies` enum. |
| `src/strategies/harvest_blend.ts` | Keeper-side harvest driver. |
| `src/deploy_adapter.ts` | Soroswap adapter deploy. Broken, see Gotchas. |
| `src/publish_addresses.ts` | Copies the internal address book into `public/`. Broken, see Gotchas. |
| `src/utils/env_config.ts` | Loads `.env` plus `configs.json` into an `EnvConfig`. |
| `src/utils/address_book.ts` | `AddressBook`, the JSON `ids` + `hashes` store. |
| `src/utils/contract.ts` | `installContract`, `deployContract`, `invokeContract`, airdrops, TTL bumps. |
| `src/utils/tx.ts` | Transaction assembly, signing, budget extraction. |
| `src/utils/vault.ts` | 31 exported vault-facing helpers and role constants used by the on-chain tests. |
| `src/utils/deploy_tools.ts` | `Strategies` / `Assets` enums and strategy init-arg construction. |
| `src/tests/` | On-chain smoke tests. `mainTest.ts` is the entry point. |
| `bash/deploy-blend-strategy.sh` | Interactive mainnet Blend strategy deploy via the `stellar` CLI. |
| `configs.json` | Per-network RPC URLs and vault role assignments. |
| `public/*.contracts.json` | Checked-in deployed addresses and WASM hashes. |

## Public surface

`package.json` scripts (`package.json:5`-`:19`). Every script except `build` compiles with `tsc` first and runs the emitted file from `dist/`.

| Script | Command | Args |
|---|---|---|
| `build` | `docker-compose run --rm --no-deps defindex-soroban make build` (`package.json:6`) | Requires a `docker-compose.yml` that is **not in this repo**. |
| `deploy-factory` | `node dist/deploy_factory.js` (`package.json:7`) | `<network>` |
| `deploy-blend` | `node dist/strategies/deploy_blend.js` (`package.json:8`) | `<network> [ASSET_SYMBOL]` |
| `deploy-strategies` | `node dist/strategies/deploy_strategies.js` (`package.json:9`) | `<network> <asset> [count] [force_install]` |
| `deploy-vault` | `node dist/deploy_vault.js` (`package.json:10`) | `<network> <asset>` |
| `multi-deploy-blend` | `node dist/strategies/multi_deploy_blend.js` (`package.json:11`) | `<network> <count> <asset_key>` |
| `publish-addresses` | `node dist/publish_addresses.js` (`package.json:12`) | `<network>` |
| `test` | `node dist/tests/mainTest.js` (`package.json:13`) | `<network> <flag>` |
| `dev-blend`, `test-blend-strategy`, `test-blend-vault`, `test-deployed-blend-vault` | `package.json:14`-`:17` | Blend-specific drivers |
| `count-lines` | `package.json:18` | |
| `vault-example` | `node dist/vault_usage_example.js` (`package.json:19`) | |

`mainTest.ts` flags (`src/tests/mainTest.ts:137`): `-h` help, `-a` all, `-oaos` one asset one strategy, `-oats` one asset two strategies, `-taos` two assets one strategy, `-tats` two assets two strategies.

Rust build entry points, all requiring the `stellar` CLI on PATH: `make build` / `make test` / `make fmt` / `make clean` at the repo root (`Makefile:1`, subdirs `strategies vault factory integration-test`).

## Key methods

- **`EnvConfig.loadFromFile(network)`** (`src/utils/env_config.ts:90`): merges `configs.json` with `.env`. The RPC URL comes from `MAINNET_RPC_URL` on mainnet and from `configs.json`'s `soroban_rpc_url` everywhere else (`src/utils/env_config.ts:137`-`:145`). It then validates that every required field is present **and non-empty** (`src/utils/env_config.ts:149`-`:166`), so the `standalone` and `futurenet` entries in `configs.json` fail today because their role fields are blank strings. The admin keypair is `DEPLOYER_SECRET_KEY`, non-null-asserted (`src/utils/env_config.ts:175`), so a missing key throws inside `Keypair.fromSecret`.
- **`AddressBook.loadFromFile(network, addressBookPath = "../../.soroban")`** (`src/utils/address_book.ts:29`): paths are resolved relative to the **compiled** file's `__dirname` under `dist/`, not to the source tree. Errors are swallowed and an empty book is returned (`src/utils/address_book.ts:42`), so a wrong path shows up later as "unable to find address for X" (`src/utils/address_book.ts:86`).
- **`AddressBook.writeToFile()`** (`src/utils/address_book.ts:51`): always creates `../../.soroban/` even when the book was loaded from `public/`, then writes back to whatever path it was loaded from.
- **`installContract` / `deployContract`** (`src/utils/contract.ts:44`, `:73`): resolve the WASM through the fixed `CONTRACT_REL_PATH` map (`src/utils/contract.ts:23`), which covers `defindex_vault`, `defindex_factory`, `soroswap_adapter`, `hodl_strategy`, `blend_strategy`, `fixed_apr_strategy`. `unsafe_hodl_strategy` and the xycloans adapter are not in the map.
- **`deployBlendStrategy`** (`src/strategies/deploy_blend.ts:134`): validates the config shape (`src/strategies/deploy_blend.ts:31`), refuses to run below 35 XLM of admin balance (`src/strategies/deploy_blend.ts:153`), pre-checks that the admin holds enough of each asset for the bootstrap deposit (`src/strategies/deploy_blend.ts:177`), then per strategy deploys and immediately calls `deposit` (`src/strategies/deploy_blend.ts:220`). The deposit amount is derived from the pool's live `b_rate` as `b_rate * 1001 / 1e12 + 1` (`src/strategies/deploy_blend.ts:117`), which is the minimum that clears the strategy's 1000-share anti-inflation floor. The contract key is `<asset>_blend_<name>_<pool_name>_strategy` (`src/strategies/deploy_blend.ts:197`).
- **`constructBlendStrategyArgs`** (`src/strategies/deploy_blend.ts:85`): builds the positional `init_args` for `BlendStrategy::__constructor`. It **hardcodes `reward_threshold` to 40** (`src/strategies/deploy_blend.ts:90`) and ignores the `reward_threshold` field in `blend_deploy_config.json`. `src/utils/deploy_tools.ts:89` does the same.
- **`deployVault`** (`src/deploy_vault.ts:85`): reads the strategy address from `public/<network>.contracts.json` under `<ASSET>_<strategy>_strategy` (`src/deploy_vault.ts:46`) and the Soroswap router under `soroswap_router` (`src/deploy_vault.ts:29`), then invokes `create_defindex_vault` on the factory (`src/deploy_vault.ts:108`).
- **`bash/deploy-blend-strategy.sh`**: a `stellar` CLI path that deploys from an already-uploaded WASM hash pinned at `bash/deploy-blend-strategy.sh:39`. That hash matches `public/mainnet.contracts.json:13`. It builds the typed `init_args` JSON by hand (`bash/deploy-blend-strategy.sh:180`) and optionally writes the result into the contracts file with `jq`.

## Configuration

- **`.env`** (template at `.env.example`): `DEPLOYER_SECRET_KEY`, `BLEND_DEPLOYER_SECRET_KEY`, `BLEND_KEEPER_SECRET_KEY`, `SOROSWAP_MINT_SECRET_KEY`, `TEST_USER`, `MAINNET_RPC_URL`. Never commit real values.
- **`configs.json`**: one entry per network. Fields consumed by `EnvConfig`: `network`, `horizon_rpc_url`, `soroban_rpc_url` (non-mainnet), `friendbot_url` (non-mainnet), `soroban_network_passphrase`, `blend_keeper`, `defindex_factory_admin`, `defindex_fee_receiver`, `defindex_fee`, `vault_fee_receiver`, `vault_manager`, `vault_emergency_manager`, `vault_rebalance_manager`, `vault_name`, `vault_symbol` (`src/utils/env_config.ts:107`-`:121`). Mainnet has `defindex_fee: "2000"` (20%) at `configs.json:9`.
- **`src/strategies/blend_deploy_config.json`**: per-network `install_contract` flag plus a `strategies` array of `{ name, keeper, asset, asset_symbol, reward_threshold, blend_pool_address, blend_pool_name }` (required fields enforced at `src/strategies/deploy_blend.ts:33`-`:41`).
- **Address books.** Three layers: `.soroban/<network>.contracts.json` written by the scripts (gitignored, `.gitignore:2`), `public/<network>.contracts.json` checked in and read as the shared book, and a second `public/` lookup two levels further up used by `src/deploy_vault.ts:19`.

## Dependencies

npm (`package.json:34`-`:36`): `@stellar/stellar-sdk ^14.0.0`, `dotenv ^16.4.5`. Dev: `typescript ^5.3.3`, `ts-node`, `eslint`, `prettier`, `@stellar/tsconfig`.

External services and contracts referenced: Stellar Horizon and Soroban RPC (`configs.json`), Friendbot on testnet and futurenet, the Soroswap router (`soroswap_router` key), Blend pools (`blend_pool_*` keys and `blend_deploy_config.json`), and `https://rpc.lightsail.network` hardcoded in `bash/deploy-blend-strategy.sh:36`.

Calls into: the DeFindex **factory** (`create_defindex_vault`) and **vault** contracts documented in [`factory.md`](factory.md) and [`vault.md`](vault.md), and the **Blend strategy** (`deposit`, `harvest`) in [`blend-strategy.md`](blend-strategy.md).

## Gotchas & invariants

- **`src/publish_addresses.ts` is broken in this repo.** It hardcodes `/workspace/apps/contracts/.soroban` and `/workspace/public` (`src/publish_addresses.ts:16`-`:17`), absolute paths from the old `paltalabs/defindex` monorepo and its docker workspace. `yarn publish-addresses` will just warn that the source file does not exist. Copy the address book by hand.
- **`bash/deploy-blend-strategy.sh:48` resolves `public/mainnet.contracts.json` two directories too high** (`$SCRIPT_DIR/../../../public/...`). In this standalone repo the correct path is `$SCRIPT_DIR/../public/...`. The deploy itself works; the optional "store this address" step will fail with "contracts file not found".
- **The README's paths are pre-split.** It refers to `apps/contracts/`, `docker compose up -d`, `bash run.sh` and `apps/rust_docs/` (`README.md:8`, `:13`, `:89`, `:299`). None of those exist here. `package.json:6` likewise calls `docker-compose` with no compose file in the repo. Build with `make build` directly, given a Rust toolchain with the `wasm32v1-none` target and the `stellar` CLI.
- **`yarn test testnet -bs` and `-bv` are advertised but disabled.** The help text still lists blend strategy and blend vault tests (`src/tests/mainTest.ts:156`-`:157`) while both cases are commented out (`src/tests/mainTest.ts:254`-`:279`). They fall through to `default` and exit silently.
- **`src/deploy_adapter.ts` cannot work.** It installs the WASM key `soroswap_adapter` (`src/deploy_adapter.ts:22`), which maps to `soroswap_adapter.optimized.wasm` (`src/utils/contract.ts:29`), but the crate builds `soroswap_strategy.optimized.wasm`. It also hardcodes a router address (`src/deploy_adapter.ts:24`) and has no `package.json` script.
- **`reward_threshold` in `blend_deploy_config.json` is decorative.** Both TypeScript paths hardcode 40. Only `bash/deploy-blend-strategy.sh` honors a supplied value. The README already flags this (`README.md:181`).
- **`configs.json` `standalone` and `futurenet` entries are unusable as committed.** Their role fields are empty strings and `EnvConfig` rejects any field shorter than one character (`src/utils/env_config.ts:158`).
- **Mainnet has no `soroban_rpc_url` in `configs.json`.** That is intentional: mainnet reads `MAINNET_RPC_URL` from `.env` (`src/utils/env_config.ts:138`). The README's config table does not mention this split.
- **Strategy address keys are case-inconsistent across networks.** `public/testnet.contracts.json` uses uppercase (`USDC_blend_strategy`, line 4) while `public/mainnet.contracts.json` uses lowercase with a pool suffix (`usdc_blend_autocompound_fixed_strategy`, line 4). `src/deploy_vault.ts:46` builds the key from the raw CLI argument, so the asset argument's casing matters.
- **`public/deprecated.contracts.json` holds retired yieldblox strategy addresses.** Do not treat it as live.
- **`node_modules/` is not in `.gitignore`** (`.gitignore` lists only `target`, `.soroban`, `**/test_snapshots/**`, `.vscode`). Be careful before `git add -A`.

## Testing

`src/tests/` runs against a live network, not in a sandbox. It needs a funded `DEPLOYER_SECRET_KEY`, a `TEST_USER`, deployed factory and strategies, and on testnet a USDC trustline (`src/utils/vault.ts:142`).

Sequence from the README (`README.md:54`-`:62`): `make build`, `yarn deploy-factory <network>`, `yarn deploy-strategies <network> <asset> 2 true`, `yarn publish-addresses <network>` (broken, copy by hand), `yarn deploy-vault <network> <asset>`, then `yarn test <network> -a`.

For deterministic, offline coverage use the Rust suites instead: see [`integration-test.md`](integration-test.md).
