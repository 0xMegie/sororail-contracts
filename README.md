# SoroRail — contracts

**Composable Soroban payment contracts.** Rust, `no_std`, audited-in-intent.

This is the core repository of [SoroRail](https://github.com/sororail): a small
set of well-tested payment primitives that recur in nearly every real Stellar
application — escrow, streaming, vesting, subscriptions, batch payout — so that
teams stop hand-rolling their own vesting math and authorization checks.

The `frontend` repo — SDK, reference application and docs site — depends on
this one. Nothing downstream can be correct until these are.

> ### Unaudited. Testnet only.
>
> **Do not deploy to mainnet or handle real value until an external audit is
> complete.** No mainnet addresses will be published before then. See
> [SECURITY.md](SECURITY.md).

The full org build specification lives in [SPEC.md](SPEC.md).

## The contracts

| Crate | Purpose |
|---|---|
| `sororail-common` | Shared errors, TTL helpers, auth guards, events, checked math. Not a contract. |
| `sororail-escrow` | Funds held by the contract, released on a condition. Optional arbiter with dispute and split resolution. |
| `sororail-stream` | Continuous per-second transfer from sender to recipient. |
| `sororail-vesting` | Scheduled release against a schedule, with a cliff. |
| `sororail-recurring` | Pull-based authorization for subscriptions. |
| `sororail-batch-payout` | One transaction, many recipients. The payroll primitive. |

Each is independently deployable. Composition happens at the call site, not
through inheritance.

## Layout

```
contracts/
├── Cargo.toml              # workspace root
├── rust-toolchain.toml     # pinned toolchain
├── Makefile                # build / test / fmt / lint / optimize / specs
├── contracts/
│   ├── common/             # shared library crate
│   ├── escrow/
│   ├── stream/
│   ├── vesting/
│   ├── recurring/
│   └── batch_payout/
└── tests/                  # cross-contract integration tests
```

Every contract crate follows the same internal shape: `lib.rs`, `contract.rs`,
`storage.rs`, `types.rs`, `errors.rs`, `events.rs`, `test.rs`.

## Toolchain

Verified against current sources on 2026-09-07:

| Component | Version | Note |
|---|---|---|
| `soroban-sdk` | **27.0.6** | Latest stable. 28.0.0-rc.1 is a prerelease — do not pin it. |
| `stellar` CLI | 27.1.0 | Renamed from `soroban`; older tutorials use the old command shapes. |
| Rust | 1.97.1 | Pinned in `rust-toolchain.toml`. soroban-sdk 27 needs ≥1.91. |
| WASM target | `wasm32v1-none` | **Not** `wasm32-unknown-unknown`, which soroban-sdk ≥22 no longer targets. |

## Development

```bash
make build      # release wasm for all contracts
make test       # host tests (testutils needs std, so tests do not run on wasm)
make lint       # clippy, warnings denied
make optimize   # strip and shrink each wasm
make specs      # contract specs as JSON, for the SDK and releases
make ci         # everything CI runs
```

`cargo-nextest` is the test runner in CI. Install it locally with
`cargo install cargo-nextest`.

## Design rules

**Money math errors; it never saturates.** Every arithmetic operation goes
through `sororail_common::math`, which returns an error on overflow rather than
producing a wrong-but-plausible balance. `overflow-checks` is on in release
builds too.

**Error numbers are ABI.** `sororail_common::errors::Error` assigns each
contract a documented numeric range. A released variant is never renumbered and
never removed — clients decode failures by integer.

**Membership is checked before `require_auth`.** Missing authorization checks
are the most common Soroban vulnerability class, so guards live in
`sororail_common::auth` and every privileged entry point has a test asserting it
fails for an unauthorized caller.

**State expires.** Soroban archives entries that are not extended. Mutating
entry points extend the instance TTL, while read-only views do not write a
ledger entry. Integrators operating grants, streams, subscriptions, or
escrows longer than 30 days should schedule the contract's public `bump()`
entry point before expiry. A caller pays the normal transaction and ledger
write fees; if the instance has already been archived, it must first be
restored through the network's archival-state restoration flow.


## Contract Immutability and Migration

Soroban contracts are immutable once deployed. This means that contract logic cannot be directly upgraded. Any changes to a contract (bug fixes, feature additions) require deploying a new contract instance.

#### Migration Plan:

When a new version of a SoroRail contract is deployed, existing contracts will continue to function under their original logic. Users or integrated applications will need to:

1.  **Deploy New Instances:** New instances of the upgraded contracts must be deployed.
2.  **Migrate Funds/State:** For contracts that hold funds or state (e.g., `escrow`, `stream`, `vesting`, `recurring`), a migration strategy is necessary. This typically involves:
    *   **Draining Old Contracts:** Encouraging users to withdraw all funds or complete all operations on the old contract instance.
    *   **New Interactions:** Directing new interactions and deployments to the new contract instances.
    *   **Wrapper Contracts/SDK Updates:** Providing wrapper contracts or SDK updates to facilitate interaction with the new contract versions, potentially abstracting away the underlying contract address changes.

Due to the immutable nature, careful planning and communication are essential for any contract upgrades to ensure a smooth transition for users.

## Testing requirements

Non-negotiable — this is what makes the contracts credible as a dependency:

- Unit tests per contract using `soroban_sdk::testutils`, advancing time via the
  test ledger rather than a mocked clock.
- An authorization test per privileged entry point, asserting failure for an
  unauthorized caller.
- Arithmetic edge cases: zero amounts, `i128::MAX`, one-second durations, cliff
  equal to duration, stop before start.
- Conservation invariants for `stream` and `vesting`: withdrawn + refunded +
  remaining always equals deposited, exactly, with no rounding leakage.
- Integration tests in `tests/` deploying real token contracts over full
  lifecycles.
- ≥90% line coverage plus region/function floors, enforced in CI.

## Status

`v0.1` in progress. See [SPEC.md](SPEC.md) for the roadmap.

- [x] `common`
- [x] `escrow`
- [x] `stream`
- [x] `vesting`
- [x] `recurring`
- [x] `batch_payout`
- [x] Cross-contract integration tests in `tests/`
- [x] Coverage measured: 98.66% lines, against the ≥90% gate
- [x] Testnet deployment, addresses recorded in [DEPLOYMENTS.md](DEPLOYMENTS.md)
- [ ] `MAX_RECIPIENTS` re-measured against testnet (see below)
- [ ] Crate names reserved on crates.io — tracked per-crate in
      [DEPLOYMENTS.md](DEPLOYMENTS.md#cratesio-publication)

### Known gaps

**`batch_payout`'s recipient cap is measured, but only locally.** The ramp in
`report_batch_ceiling` found 40 recipients execute and 45 exceed the budget
under soroban-sdk 27.0.6, so `MAX_RECIPIENTS` is 40. The local environment does
not model transaction size limits, and its `mock_all_auths` builds one
authorization entry per transfer where a real submission signs a single tree —
so the on-chain figure may differ in either direction. Re-run against testnet
before `v0.2`.

**Every contract is one position per deployed instance.** One escrow, one
stream, one grant, one subscription per contract. That follows the entry-point
signatures in [SPEC.md](SPEC.md), which take no position id. It is worth
revisiting: an id-keyed design would let a single deployment hold many
positions, which matters for the payroll and dashboard screens the app is
meant to have.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Issues labelled `good-first-issue` are
scoped so that someone new to Soroban can complete them.

## License

Apache-2.0. See [LICENSE](LICENSE).


## Updates

Updated documentation for recent changes.
