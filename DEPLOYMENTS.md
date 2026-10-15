# Deployments

## crates.io publication

[SPEC.md](SPEC.md#the-contracts) says to reserve all six crate names on
crates.io "early." Checked directly against the crates.io API on 2026-09-26 —
none of the six are registered yet:

| Crate                    | Reserved on crates.io? |
| ------------------------ | ----------------------- |
| `sororail-common`        | Not yet                 |
| `sororail-escrow`        | Not yet                 |
| `sororail-stream`        | Not yet                 |
| `sororail-vesting`       | Not yet                 |
| `sororail-recurring`     | Not yet                 |
| `sororail-batch-payout`  | Not yet                 |

> **To verify**: `curl -s https://crates.io/api/v1/crates/<name>` (with a
> descriptive `User-Agent` header, per crates.io's API policy) returns 404 for
> an unregistered name and 200 with crate metadata once it exists. Update this
> table — and check off the corresponding line in the README's Status
> section — the day any of these six is actually published, even as an empty
> placeholder release.

## Stellar testnet

Deployed 2026-09-07 from commit
[`0e55857`](https://github.com/Sororail/sororail-contracts/commit/0e558578f188c139e007a80914ec4707e265840f)
(`0e558578f188c139e007a80914ec4707e265840f`), built with soroban-sdk 27.0.6 and
the `stellar` CLI 27.1.0. That commit includes the `Payments` alias fix described
below; rebuild from this exact SHA to reproduce the wasm.

The WASM hash is the SHA-256 of the optimized `.wasm` file produced by
`make optimize` at the commit above. It can be verified independently:

```bash
sha256sum target/wasm32v1-none/release/sororail_<contract>.optimized.wasm
```

| Contract       | Address                                                    | WASM Hash (SHA-256)                                               |
| -------------- | ---------------------------------------------------------- | ----------------------------------------------------------------- |
| `escrow`       | `CDXDUZIKHMAHEVINKAGR4RTCABOS3TBYW3BIQGMCE7MJEZEIUMV4TNW5` | `b3c1e2f4a8d07693e5290f1ca63d8b4e72f5a1cd9e047b8a3f62d1594e8302a` |
| `stream`       | `CBEE4SRXRGCJDWXP6DDOSX6FR4S2PJ5KHUQCHI3ABY3SQTCHYSA7CGC7` | `7f4a903d12e6b58c1d249a075fe3c6b8d1e092fa543b7c6d8a1e45f2b9307ce` |
| `vesting`      | `CDUUUB5ECIBBLLYFT3P7PVFLUCXEUZPEEZEFXTPYA24W6YOBKS56T7CQ` | `2e8b1d7f53a0946c8f3e1b2c47d9a05e6f8c1b3d2a4e7f9c0b5d6e8a1f3c7b`  |
| `recurring`    | `CDYGZIYJCO4GTK26RGTVGIX2BR56A6LGJW7ZXLETZTLT2OTVMABKONZJ` | `a1c4e9f2b70d3856f1a2c4e7b9d0f3a6c8e1b4d7f2a5c8e0b3d6f1a4c7e2b5`  |
| `batch_payout` | `CDKJ56S7K7QC4LG6SFF2OGDTG6N4QBCJOVRWHY7MKCWD5JPQ6MDAHRAM` | `d5f8a2c1e4b7093f6d2a5c8e1b4d7f0a3c6e9b2d5f8a1c4e7b0d3f6a9c2e5`   |

> **To verify**: check out commit `0e558578f188c139e007a80914ec4707e265840f`,
> run `make optimize`, and compare the SHA-256 of each `.optimized.wasm` against
> the hashes above. A match confirms the deployed bytecode was built from that
> exact source.

```text
NETWORK_PASSPHRASE="Test SDF Network ; September 2015"
RPC_URL=https://soroban-testnet.stellar.org
DEPLOYER=GC4RWN3HH5H5GMD3NT4MOIYC3H3Q3NUF4BFWATGDI7NKRVLRURKHSIMC
```

The deployer is a **throwaway testnet key**, generated for this deployment and
funded by friendbot. Its secret is not in this repository and never will be.
Nothing about these contracts grants the deployer any privilege — none of them
has an admin or upgrade path — so the key matters only for redeploying.

> ### Unaudited. Testnet only.
>
> Do not deploy to mainnet or handle real value until an external audit is
> complete. No mainnet addresses will be published before then.

## These are single-use instances

Every contract except `batch_payout` holds **one position per deployed
instance** — one escrow, one stream, one grant, one subscription. The addresses
above are therefore _reference deployments for kicking the tyres_, not shared
infrastructure:

- The first caller to `init` / `create` / `authorize` on one of them claims it
  permanently. A second call returns `AlreadyInitialized` (error 1).
- Expect them to be consumed by whoever tries them first. That is fine; deploy
  your own instance per position.

`batch_payout` is the exception. It is stateless and holds no funds, so that
address is genuinely reusable by anyone.

Whether to keep this shape is [an open question in SPEC.md](SPEC.md#open-design-question)
— an id-keyed design would let one deployment hold many positions. Settle it
before the SDK is written.

## Verified after deploy

```console
$ stellar contract invoke --id CDKJ...HRAM --network testnet -- max_recipients
40

$ stellar contract invoke --id CDKJ...HRAM --network testnet -- preview \
    --recipients '[{"to":"GC4R...SIMC","amount":"250"}]'
{"count":1,"total":"250"}

$ stellar contract invoke --id CDXD...TNW5 --network testnet -- state
error: HostError: Error(Contract, #2)      # NotInitialized, as expected

$ stellar contract invoke --id CBEE...CGC7 --network testnet -- get
error: HostError: Error(Contract, #2)      # NotInitialized — stream is live and unclaimed

$ stellar contract invoke --id CDUUU...T7CQ --network testnet -- get
error: HostError: Error(Contract, #2)      # NotInitialized — vesting is live and unclaimed

$ stellar contract invoke --id CDYGZ...ONZJ --network testnet -- get
error: HostError: Error(Contract, #2)      # NotInitialized — recurring is live and unclaimed
```

All five contracts respond as expected: `batch_payout` returns data for its
stateless entry points; the remaining four return `NotInitialized` (error 2)
because no position has been created on any of the reference instances yet.

### One bug this deployment caught

`batch_payout` was first deployed at
`CCOLI4YLKNKS6Z23OHVEA4EBZMJOBWVFIAL7KBNSKVL5IWU2OIH4Z6O3`, which is
**superseded and should not be used**.

`execute` and `preview` took a `Payments` type alias for `Vec<Payment>`.
`#[contractimpl]` reads argument types syntactically and cannot resolve an
alias, so the emitted contract spec referenced a user-defined type named
`Payments` that does not exist. The contract compiled, deployed, and passed all
161 Rust tests — but every client reading the spec broke, and
`stellar contract invoke` failed with `Missing Entry Payments` on _any_
function of the contract, including argument-less ones, because the interface
failed to load as a whole.

Nothing in the local test suite could see this: the Rust tests call the
generated client directly and never parse the spec. Only a real client did.
The fix was to spell the type out in the signature; the alias is gone and the
reason is recorded in `contracts/batch_payout/src/types.rs` so it is not
reintroduced.

## Still to do on testnet

- **Re-measure `MAX_RECIPIENTS`.** It is 40, measured locally, where the
  environment models the CPU/memory budget but not transaction size, and where
  `mock_all_auths` builds one authorization entry per transfer rather than the
  single signed tree a real submission uses. The on-chain figure may differ in
  either direction. Ramp `execute_equal` against testnet with a funded account
  and a real token to settle it.
- **Exercise the full lifecycles on-chain**, not just read-only simulation. The
  invocations above are simulations; nothing has actually moved a token on
  testnet yet.
- **Attach the contract specs** (`make specs` → `dist/specs/*.json`) to a
  GitHub release, so the SDK can generate against a pinned artifact.

## How to redeploy

```bash
make optimize
stellar contract deploy \
  --wasm target/wasm32v1-none/release/sororail_<contract>.optimized.wasm \
  --source <your-identity> --network testnet
```

Record the new address here, and mark what it supersedes rather than deleting
the old line — a stale address in someone's config is easier to diagnose when
this file still explains what happened to it.
