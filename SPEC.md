# SoroRail

**Payment rails for Stellar.** Audited Soroban contracts, a typed client SDK, and a reference application — so that teams building payroll, subscriptions, escrow, or streaming on Stellar stop rewriting the same contracts from scratch.

> **This document is a build specification.** It describes a GitHub organization across two repositories. Build one repository at a time and finish it before starting the next.

> **Revision note (2026-09-07).** This spec originally described _five_ repositories — `contracts`, `sdk`, `app`, `docs` and `.github`. It has been consolidated to **two** at the maintainer's direction: `sdk`, `app` and `docs` are now one `frontend` repo, and `.github`'s templates are copied into each repo instead of inherited. Two contradictions in the original were also fixed: it named a `token_utils` crate that appeared nowhere else (the shared crate is `common`), and it gave two different build orders. Sections describing already-built code have been updated to describe what was actually built.

---

## Table of contents

- [Why this exists](#why-this-exists)
- [Organization structure](#organization-structure)
- [Build order](#build-order)
- [Repo 1 — `contracts`](#repo-1--contracts)
- [Repo 2 — `frontend`](#repo-2--frontend)
- [Conventions that apply to both repos](#conventions-that-apply-to-both-repos)
- [Security posture](#security-posture)
- [Roadmap](#roadmap)
- [Verify before you build](#verify-before-you-build)

---

## Why this exists

Across the Stellar ecosystem the same payment logic is being reimplemented in isolation. Payroll systems, streaming protocols, group-settlement engines, and escrow marketplaces each hand-roll their own vesting math, their own authorization checks, their own batch disbursement loops. Each implementation carries its own bugs, and none of them are audited.

SoroRail is the shared layer underneath all of that: a small set of composable, well-tested Soroban contracts covering the payment patterns that recur in nearly every real application, plus a typed TypeScript client so frontend developers never touch XDR by hand.

The design target is `openzeppelin-contracts` for Stellar payments — boring, correct, obvious to audit, and easy to depend on.

**On the name.** A rail is the boring, load-bearing thing underneath a payment system that nobody thinks about until it breaks. That is the ambition. The `soro-` prefix marks it as Soroban-native; note that SDF increasingly brands the platform "Stellar smart contracts" in product surfaces while retaining "Soroban" in developer documentation, so prefer "Stellar smart contracts" in user-facing copy and "Soroban" in developer-facing copy.

**Non-goals.** This is not a wallet. It is not an anchor or an on/off-ramp. It does not do KYC, custody, fiat rails, or DEX routing. It does not issue tokens. Scope discipline is the point; proposals that expand this list should be declined.

---

## Organization structure

GitHub organization: **`sororail`**

| Repo        | Language       | Purpose                                                | Depends on  |
| ----------- | -------------- | ------------------------------------------------------ | ----------- |
| `contracts` | Rust / Soroban | The payment contracts. The core deliverable.           | —           |
| `frontend`  | TypeScript     | The SDK, the reference application, and the docs site. | `contracts` |

```
      contracts  (Rust, Soroban)
          │
          │  publishes: WASM + contract specs to GitHub Releases
          ▼
      frontend  (TypeScript)
      ├── packages/sdk    → @sororail/sdk on npm
      ├── apps/web        → Next.js reference application
      └── apps/docs       → Astro Starlight documentation site
```

Each repo is separately claimable on Drips and carries its own `FUNDING.json`. Suggested split for `frontend`: 70% maintainers, 30% upstream to `contracts`.

### Why two repos rather than one or five

The split that earns its keep is **Rust from TypeScript**. The toolchains share nothing, the release cadences genuinely differ — contracts should move slowly and deliberately, the app can move daily — and a Rust engineer should not be notified about a CSS change.

Splitting further was reconsidered and rejected. The SDK, the app and the docs site share a language, a package manager and a deploy story, and the docs are required to extract every code sample from a compiling example in the SDK. Keeping them in one repo with a workspace removes a cross-repo coupling that would otherwise need a published package to cross.

The cost is that `frontend` needs an internal workspace layout with real discipline about what depends on what.

---

## Build order

1. **`contracts`** — the bulk of the work. Nothing downstream can be correct until these are. ✅ **Built.**
2. **`frontend`** — consumes the published contract specs and WASM.

Within `contracts`, build one contract at a time, complete with tests, before starting the next: `common`, then `escrow`, `stream`, `vesting`, `recurring`, `batch_payout`.

Within `frontend`, build the SDK first, then the app, then the docs — the docs are written last, when the API has stopped moving.

**When prompting Claude Code:** open a separate session per repo, in that repo's own directory. Paste only that repo's section plus [Conventions](#conventions-that-apply-to-both-repos) and [Verify before you build](#verify-before-you-build). Feeding the whole document produces shallow work across both repos instead of one finished repo.

---

## Repo 1 — `contracts`

> Composable Soroban payment contracts. Rust, `no_std`, audited-in-intent.
>
> **Status: built and deployed to testnet.** 161 tests passing; fmt, clippy `-D warnings` and the wasm build green. Testnet addresses recorded in `DEPLOYMENTS.md`. Not yet audited. See the repo's own README for the current gap list.

### Stack

- Rust, edition 2021, toolchain pinned in `rust-toolchain.toml`
- `soroban-sdk` **27.0.6** — the latest _stable_. Note 28.0.0-rc.1 is published to crates.io; it is a prerelease and must not be pinned.
- `stellar` CLI 27.1.0 for build, deploy and invoke
- Target **`wasm32v1-none`**, not `wasm32-unknown-unknown` — soroban-sdk ≥22 no longer targets the latter
- `cargo-nextest` as the CI test runner
- Workspace layout: one crate per contract, plus one shared crate
- Crates published to crates.io as `sororail-common`, `sororail-escrow`, `sororail-stream`, `sororail-vesting`, `sororail-recurring`, `sororail-batch-payout`. Reserve all six names early.

### Layout

```
contracts/
├── Cargo.toml                 # workspace root
├── rust-toolchain.toml        # pin the toolchain
├── Makefile                   # build / test / fmt / lint / optimize / specs
├── FUNDING.json
├── contracts/
│   ├── common/                # shared types, errors, events, guards
│   ├── escrow/
│   ├── stream/
│   ├── vesting/
│   ├── recurring/
│   └── batch_payout/
├── tests/                     # cross-contract integration tests
└── .github/workflows/ci.yml
```

Each contract crate follows the same internal shape: `lib.rs`, `contract.rs`, `storage.rs`, `types.rs`, `errors.rs`, `events.rs`, `test.rs`. `batch_payout` is the one exception — it is stateless and so has no `storage.rs`.

### The contracts

Each is independently deployable; composition happens at the call site, not through inheritance.

#### `common`

Not a contract — a shared library crate.

- `Error` enum with a stable, documented numeric mapping, one shared enum across the org with a reserved range per contract. **Never renumber or remove a released variant**; clients decode failures by integer.
- Storage key enums and TTL/bump helpers. Soroban state expires; mutating
  operations extend TTL, while views remain read-only. Long-lived positions
  expose `bump()` so an integrator can keep them alive.
- `require_auth` guard helpers. These check **membership before calling `require_auth`** — the reverse order would prompt an address that was never permitted to act.
- Basis-point math with explicit overflow handling. No silent saturation. `split_bps` derives the remainder by subtraction so splits conserve the total exactly.

Note on events: the original spec called for shared event-emission helpers here. `env.events().publish` is **deprecated as of soroban-sdk 27** in favour of the `#[contractevent]` macro, which is applied at each event's own definition site and so cannot be wrapped by a shared function. `common::events` therefore documents the normative topic scheme instead of providing code.

#### `escrow`

Funds held by the contract, released on a condition.

- `init(depositor, beneficiary, arbiter: Option<Address>, token, amount, deadline)`
- `fund()` — pulls tokens from depositor
- `release(caller)` — beneficiary receives; callable by depositor or arbiter
- `refund(caller)` — callable by depositor after deadline, or by arbiter at any time. If in `Disputed` state, callable by anyone after 7 days past the original deadline to recover from an unresponsive arbiter.
- `dispute(caller)` / `resolve(split_bps)` — arbiter splits between parties; `split_bps` is the beneficiary's share
- `dispute` requires a configured arbiter; calling it without one returns `EscrowNoArbiter`, so `Disputed` is never reachable without an arbiter. `resolve` re-checks independently as defense-in-depth.
- Calling `dispute` on an escrow that is already in `Disputed` state returns `EscrowAlreadyDisputed`. Once disputed, a second `dispute` call is rejected so the arbiter cannot be overridden mid-resolution.
- `fund` requires `deadline` not to have passed (inclusive); past it, it errors with a dedicated `DeadlinePassed` rather than the generic `EscrowClosed`, since the escrow is still `Created`, not terminal.
- States: `Created → Funded → (Released | Refunded | Disputed → Resolved)`. Illegal transitions must error, not panic.
- **Dispute timeout:** Once the deadline passes, an unresolved dispute has a 7-day grace period. After this period, either party (or anyone) may refund to recover funds from an unresponsive arbiter. This is a safety mechanism, not a normal flow.

#### `stream`

Continuous per-second transfer from sender to recipient.

- `create(sender, recipient, token, rate_per_second, start, stop, cancellable: bool)` — positional arguments on purpose, not a `CreateStreamParams` struct: Soroban allows a `#[contracttype]` params object (and that would clear `clippy::too_many_arguments`), but a flat ABI matches the SDK's uniform method shape without a nested params type per entry point. Same decision applies to `vesting::create`.
- `withdraw(amount: Option<i128>)` — recipient claims accrued balance; `None` claims all available
- `cancel()` — settles accrued amount to recipient, returns remainder to sender; only if `cancellable`
- `balance_of(who)` — view; accrued but unwithdrawn
- `top_up(amount)` / `extend(new_stop)`
- Accrual is computed from ledger timestamp at read time. Never write per-second state.
- The critical correctness property: withdrawn + refunded + remaining always equals deposited, exactly, with no rounding leakage. Test this as an invariant.
- The stream is fully funded for its whole declared span, so `deposited == rate * (stop - start)` always holds. `top_up` therefore rejects an amount that is not a whole number of seconds rather than stranding the remainder.
- Unlike `vesting`'s proportional math, this contract's math never divides at all — accrual is pure multiplication (`rate * elapsed`) — so "no rounding leakage" holds by construction, not by a rounding-direction rule. There is no rounding direction to pick here.

#### `vesting`

Scheduled release against a schedule, with a cliff.

- `create(grantor, beneficiary, token, total, start, cliff, duration, revocable: bool)`
- `claim()` — beneficiary withdraws vested-but-unclaimed
- `revoke()` — grantor reclaims unvested; vested portion stays claimable
- `vested_amount(at: u64)` — pure view, testable in isolation
- `schedule()` — the schedule as a list of `Checkpoint { at, vested }` corners: start, cliff, and end (cliff and end collapse when they coincide). Linear between consecutive checkpoints. Reports the schedule as created, so it is unaffected by revocation.
- `create` rejects `grantor == beneficiary` with `IdenticalParties` before any funds move.
- Linear vesting after cliff. Nothing claimable before cliff. Fully vested at `start + duration`.
- `cliff` and `duration` are spans in seconds from `start`, not absolute timestamps. `cliff == duration` is a legal all-or-nothing unlock.
- Grants are immutable once created — no `top_up` or `extend` like `stream` provides. Issue a new grant for additional allocations.

#### `recurring`

Pull-based authorization for subscriptions. The payer authorizes a cap and cadence; the payee pulls within it.

- `authorize(payer, payee, token, amount_per_period, period_seconds, max_periods: Option<u32>)` — rejects `payer == payee` with `IdenticalParties`
- `charge()` — callable by payee, at most once per elapsed period
- `cancel(caller)` — callable by either party, effective immediately
- `next_chargeable_at()` — view
- Must not accrue chargeable periods retroactively when a payee skips one. A payee who forgets to charge for three months cannot then charge three times. This is a deliberate consumer-protection choice; document it prominently.
- Consequence for automated payees: whoever drives `charge` on a schedule (for example the scheduled recurring runs in [`apps/web`'s Payroll feature](#appsweb)) gets no catch-up after downtime, a failed cron, or a skipped job. The next successful `charge` collects exactly one period however many have elapsed, and every other elapsed period is forfeited. A late charge also restarts the cadence from the moment it lands (`next_chargeable_at = now + period_seconds`), so repeated lateness drifts the schedule. The rationale is documented on `recurring::Authorization`.
- The pull works through the token's allowance: the payer `approve`s this contract as spender and `charge` uses `transfer_from`. The contract holds no funds, and revoking the allowance stops charges without touching the contract.

#### `batch_payout`

One transaction, many recipients. The payroll primitive.

- `execute(funder, token, recipients: Vec<Payment>)`
- `execute_equal(funder, token, recipients: Vec<Address>, amount_each: i128)`
- `preview(recipients)` — totals and validates without moving anything, for a pre-signature confirmation screen
- Enforce a documented maximum recipient count determined by resource limits, **discovered empirically and asserted in a test — not guessed**.
- All-or-nothing. A single failed transfer reverts the batch.

On the cap: an initial guess of 100 failed with `Error(Budget, ExceededLimit)`. The measured figure is **40** (45 exceeds the budget) under soroban-sdk 27.0.6 in the local test environment, and a CI test re-measures it so the constant cannot drift above what actually executes. The local environment does not model transaction size limits, so re-measure against testnet before `v0.2`.

### Testing requirements

Non-negotiable, and the main thing that makes this credible as a dependency:

- Unit tests per contract using `soroban_sdk::testutils`, with time advanced via the test ledger rather than mocked clocks.
- Authorization tests: every privileged entry point must have a test asserting it fails for an unauthorized caller. Missing auth checks are the most common Soroban vulnerability class.
- Arithmetic edge cases: zero amounts, `i128::MAX`, one-second durations, cliff equal to duration, stop before start.
- Conservation invariants for `stream` and `vesting`, as described above.
- Integration tests in `/tests` that deploy real token contracts and exercise full lifecycles.
- Target ≥90% line coverage, enforced in CI, with additional region (≥75%) and
  function (≥80%) floors so error-path branches cannot hide behind high line
  counts. CI also audits that every `Error::*` variant is named in the test corpus.

### Cross-contract design conventions

Each contract is internally consistent, but a few idioms differ between contracts on purpose. They are recorded here so a contributor moving between crates does not read a deliberate difference as drift.

#### Authorization: `caller` argument vs. `require_auth` on a fixed party

The rule is set by how many parties may call an entry point:

| Permitted callers       | Signature            | Check                                                             | Wrong caller gets                      |
| ----------------------- | -------------------- | ----------------------------------------------------------------- | -------------------------------------- |
| Exactly one fixed party | no `caller` argument | `stored_party.require_auth()`                                     | an authorization failure from the host |
| Two or more parties     | `caller: Address`    | an `auth::require_auth_*` guard — membership, then `require_auth` | `Error::Unauthorized`                  |

| Contract       | Single fixed party                                            | Several permitted parties (`caller`)                                             |
| -------------- | ------------------------------------------------------------- | -------------------------------------------------------------------------------- |
| `escrow`       | `init`, `fund`, `cancel` (depositor); `resolve` (arbiter)     | `release`, `refund` (depositor or arbiter); `dispute` (depositor or beneficiary) |
| `stream`       | `withdraw` (recipient); `cancel`, `top_up`, `extend` (sender) | —                                                                                |
| `vesting`      | `claim` (beneficiary); `revoke` (grantor)                     | —                                                                                |
| `recurring`    | `authorize` (payer); `charge` (payee)                         | `cancel` (payer or payee)                                                        |
| `batch_payout` | `execute`, `execute_equal` (funder)                           | —                                                                                |

A `caller` argument exists only when the contract cannot otherwise know who is acting. It also records who acted in the emitted event (`released_by`, `refunded_by`, `raised_by`, `cancelled_by`). This is why `Unauthorized` appears only in the `escrow` and `recurring` error tables. Each contract's `errors.rs` explains why the variants it never returns are absent. The contributor-facing rules for picking a pattern are in [CONTRIBUTING.md](CONTRIBUTING.md#choosing-an-authorization-pattern).

#### Authorization model, by entry point

Every public function, and which address(es) may call it. "—" means the function is a read-only view with no authorization check at all.

| Contract       | Function               | May call it                                                    |
| -------------- | ----------------------- | --------------------------------------------------------------- |
| `escrow`       | `init`                  | depositor                                                       |
| `escrow`       | `cancel`                | depositor                                                       |
| `escrow`       | `fund`                  | depositor                                                       |
| `escrow`       | `release`               | depositor or arbiter                                            |
| `escrow`       | `refund`                | depositor or arbiter (anyone, after the dispute grace period)   |
| `escrow`       | `dispute`               | depositor or beneficiary                                        |
| `escrow`       | `resolve`               | arbiter                                                         |
| `escrow`       | `state`, `bump`, `get`  | —                                                                |
| `stream`       | `create`                | sender                                                          |
| `stream`       | `withdraw`              | recipient                                                       |
| `stream`       | `cancel`                | sender                                                          |
| `stream`       | `top_up`                | sender                                                          |
| `stream`       | `extend`                | sender                                                          |
| `stream`       | `balance_of`, `remaining`, `bump`, `get` | —                                               |
| `vesting`      | `create`                | grantor                                                         |
| `vesting`      | `claim`                 | beneficiary                                                     |
| `vesting`      | `revoke`                | grantor                                                         |
| `vesting`      | `vested_amount`, `claimable`, `schedule`, `remaining`, `bump`, `get` | —                      |
| `recurring`    | `authorize`             | payer                                                           |
| `recurring`    | `charge`                | payee                                                            |
| `recurring`    | `cancel`                | payer or payee                                                  |
| `recurring`    | `next_chargeable_at`, `is_chargeable`, `remaining_periods`, `bump`, `get` | —            |
| `batch_payout` | `execute`               | funder                                                          |
| `batch_payout` | `execute_equal`         | funder                                                          |
| `batch_payout` | `max_recipients`, `preview` | —                                                           |

A wrong caller on a privileged function above gets either an authorization failure from the host (single fixed party) or `Error::Unauthorized` (several permitted parties) — see the caller-vs-`require_auth` rule just above.

#### Lifecycle modeling: explicit `State` enum vs. optional timestamps

| Contract    | Lifecycle                                         | Representation                                                                     | "Still active?" helper                                |
| ----------- | ------------------------------------------------- | ---------------------------------------------------------------------------------- | ----------------------------------------------------- |
| `escrow`    | six states, several branches                      | `State` enum (`Created`, `Funded`, `Released`, `Refunded`, `Disputed`, `Resolved`) | `State::is_terminal()`                                |
| `stream`    | active → cancelled                                | `cancelled_at: Option<u64>`                                                        | `Stream::is_cancelled()`                              |
| `vesting`   | active → revoked                                  | `revoked_at: Option<u64>`                                                          | `Grant::is_revoked()`                                 |
| `recurring` | active → cancelled, or exhausted by `max_periods` | `cancelled: bool` plus `periods_charged` against the cap                           | `Authorization::is_exhausted()`, `is_chargeable_at()` |

The rule: **use an explicit `#[contracttype]` state enum once the lifecycle has more than one non-terminal state or more than one way out of a state.** Escrow has both. `Funded` can go to three different places, and `Disputed` is a non-terminal state of its own. Encoding that as a set of flags would allow impossible combinations such as "released and disputed". An enum makes illegal transitions unrepresentable and gives each one a named error.

A binary, one-way lifecycle (active, then permanently stopped) stays as an `Option<u64>` timestamp. The timestamp is data the contract needs anyway: `stream` stops accrual at `cancelled_at`, and `vesting` freezes the vested amount at `revoked_at`. An enum would therefore duplicate that field rather than replace it. `recurring` uses a plain `bool` because nothing is computed from the cancellation time. That time is emitted in the `Cancelled` event but not stored.

Conventions that hold regardless of representation:

- The "is this position still usable?" question is always answered by a named helper on the type in `types.rs` (`is_terminal`, `is_cancelled`, `is_revoked`, `is_exhausted`), never by inspecting fields inline in `contract.rs`.
- Every illegal transition returns a contract-specific typed error (`EscrowClosed`, `StreamCancelled`, `VestingRevoked`, `RecurringCancelled`), never the generic `InvalidState` and never a panic.
- If a binary lifecycle later gains a second non-terminal state (for example, a pausable stream), migrate it to a state enum rather than adding a second `Option` field. Changing a stored `#[contracttype]` is an ABI and storage-layout break, so treat it as one.

### Open design question

Every contract is currently **one position per deployed instance** — one escrow, one stream, one grant, one subscription. That follows the entry-point signatures above, which take no position id.

It is worth revisiting before the SDK is built. An id-keyed design, where one deployment holds many positions, would suit the payroll and dashboard screens the app needs and would avoid a wasm deploy per stream. Changing it after the SDK exists is expensive, so decide now.

### Deliverables

- All six crates building to optimized WASM ✅
- `make test` green ✅
- Published contract specs as JSON, attached to a GitHub Release
- Deployed to Stellar **testnet**, with addresses recorded in `DEPLOYMENTS.md`
- No mainnet deployment until an external audit is complete. State this in the README.

---

## Repo 2 — `frontend`

> The TypeScript half: SDK, reference application, and documentation site in one workspace.

### Layout

```
frontend/
├── package.json           # workspace root (pnpm)
├── packages/
│   └── sdk/               # @sororail/sdk
└── apps/
    ├── web/               # Next.js reference application
    └── docs/              # Astro Starlight
```

`apps/web` and `apps/docs` both depend on `packages/sdk` by workspace reference. The SDK is still published to npm for outside consumers; the workspace reference is what removes the cross-repo coupling internally.

### `packages/sdk`

> `@sororail/sdk` — typed TypeScript client. No React, no framework assumptions.

**Stack:** TypeScript strict mode, `@stellar/stellar-sdk` (17.0.1 at time of writing), `tsup` for dual ESM/CJS output, `vitest`, `changesets` for versioning. Reserve the `@sororail` npm scope before the first release.

**Design rules**

- Zero framework dependencies. React helpers, if ever built, go in a separate `packages/react`.
- Every contract gets a client class: `EscrowClient`, `StreamClient`, `VestingClient`, `RecurringClient`, `BatchPayoutClient`.
- Uniform method shape: build → simulate → sign → send → confirm, with each stage independently accessible. Callers who want to inspect a simulation before signing must be able to.
- Signing is injected, never performed by the SDK. Accept a `Signer` interface; ship adapters for Freighter and for a keypair signer used in tests.
- Contract errors are decoded into typed error classes with readable messages, using the numeric ranges from `sororail-common`. A user must never see a bare error code.
- Amounts cross the API boundary as `bigint`, never `number`. Provide `toStroops` / `fromStroops` helpers and document the decimals trap loudly.
- Surface `batch_payout`'s `max_recipients()` so callers can split an oversized payroll before submitting rather than having a transaction rejected.

**Layout:** `src/clients/` (one per contract), `src/signers/`, `src/errors/`, `src/types/`, `src/utils/`, plus `test/` and `examples/`.

**Testing:** unit tests with mocked RPC for building and error decoding; integration tests against testnet contracts in a separate CI job that is allowed to be slow. Every public method needs a runnable example in `examples/` — these double as documentation and as smoke tests.

### `apps/web`

> Reference fullstack application. Proves the SDK works and shows teams how to use it.

A real application, not a demo page — but its purpose is demonstrative, so favour clarity of implementation over feature breadth.

**Stack:** Next.js App Router, TypeScript, `@sororail/sdk` by workspace reference, Freighter for wallet connection, Postgres via Drizzle ORM, a background indexer polling Soroban RPC for contract events, Tailwind.

**Critical architectural rule**

**The chain is the source of truth. The database is a cache.** Balances, stream states, and vesting positions are always read from the contract for display of current state. The database exists to make history queryable and to store off-chain metadata such as recipient names, invoice notes, and email addresses. Any screen showing money must be reconcilable against chain state, and there should be a visible way to trigger that reconciliation.

**Features**

- _Wallet and account_ — connect via Freighter, network detection with a hard warning on mismatch, testnet faucet link and a clear testnet-only banner.
- _Payroll_ (`batch_payout` + `recurring`) — recipient list with CSV import, preview of total and per-recipient amounts and estimated fees before signing, batch execution with per-recipient confirmation, scheduled recurring runs. The scheduler must treat a missed run as lost, not deferred: `recurring` never accrues periods retroactively, so if downtime or a failed cron lets more than one period elapse, the next `charge` collects only one of them and the rest are permanently forfeited (see [`recurring`](#recurring)). The scheduler cannot catch up by charging repeatedly, so it should surface missed runs to the operator instead. Duplicate detection belongs here, in the CSV import — the contract deliberately allows duplicates.
- _Streams_ (`stream`) — create with a live preview of the accrual curve, dashboard of incoming and outgoing streams with balances ticking in real time, withdraw / top up / extend / cancel.
- _Vesting_ (`vesting`) — create a grant with a visual schedule showing cliff and linear ramp, beneficiary view with claimable amount and next unlock, revoke for revocable grants.
- _Escrow_ (`escrow`) — create, fund, release, refund, optional arbiter with dispute and split-resolution flow.
- _History_ — unified event feed across all contracts from the indexer, CSV export.

**Backend**

- Route handlers under `app/api/`.
- Indexer as a separate long-running process, not a serverless function. Resumable from the last processed ledger and idempotent on replay.
- Schema: `accounts`, `contacts`, `payment_runs`, `payment_run_items`, `indexed_events`, `sync_state`.
- **No private keys server-side, ever.** All signing is client-side through the wallet. Say this explicitly in the README so no contributor is tempted.

**Design direction**

The audience is finance and operations staff at small companies, plus developers evaluating the SDK. The job is to make irreversible money movements feel inspectable before they happen.

- Confirmation before consequence: every state-changing action shows exactly what will happen — amounts, recipients, fees, and what cannot be undone — before the signing prompt.
- Numbers are the interface. Set monetary figures in a face with true tabular figures so columns align, and give amounts more typographic weight than the labels around them.
- Time is a first-class dimension for streams and vesting. Show schedules as actual schedules, not as progress bars.
- Empty states state the next action.
- Errors say what happened and what to do. Never surface a raw contract error code.
- Do not reach for the default AI-design palette. Pick a palette grounded in the subject — this is a ledger tool, and legibility under scrutiny matters more than personality. Spend boldness in one place only.

**Testing:** Playwright end-to-end against testnet contracts, covering at minimum connect wallet, create stream, withdraw, create batch payout. Component tests for amount input and formatting, which is where money bugs hide. Seed script for local development.

### `apps/docs`

- Astro Starlight, deployed to Cloudflare Pages or Vercel.
- Sections: Getting started, Concepts (one page per payment primitive, explaining the pattern before the API), Contract reference, SDK reference, Guides, Security, Contributing.
- Concepts pages must be readable by someone who has never used Soroban. This is the on-ramp.
- Every code sample must be extracted from a compiling example in `packages/sdk/examples/`, not hand-written into the docs where it will rot.
- Include a page on the decimals trap and one on Soroban TTL/state expiry, since both bite every new integrator.
- Document the two consumer-facing behaviours that will otherwise surprise people: recurring charges do not accrue retroactively, and batch payouts are capped and all-or-nothing.

---

## Conventions that apply to both repos

**Licensing.** Apache-2.0 across the org. Add the license file before the first external contribution.

**Commits.** Conventional Commits. Enforced in CI.

**Branching.** Trunk-based. Short-lived branches, PRs into `main`, squash merge, linear history.

**CI must run on every PR.** Format check, lint, tests, build. Red CI blocks merge, with no exceptions for maintainers.

**Every repo gets, at minimum:** `README.md`, `LICENSE`, `CONTRIBUTING.md`, `SECURITY.md`, `FUNDING.json`, `.editorconfig`, a CI workflow, and issue/PR templates. With no `.github` repo to inherit from, these are copied into each repo; keep them in sync by hand, which at two repos is cheap.

**Label taxonomy**, applied consistently across both: `good-first-issue`, `help-wanted`, `contract`, `sdk`, `app`, `docs`, `security`, `breaking`, and difficulty labels `size/s`, `size/m`, `size/l`.

**`FUNDING.json`** goes on the default branch of every repo from the first commit, so each repo is claimable on Drips and can accrue before anyone has heard of it.

**Issue hygiene.** Every issue intended for outside contributors must state: the problem, the affected file paths, the expected behavior, acceptance criteria, and how to test the change locally. An issue that does not meet that bar is not ready to be labelled `good-first-issue`.

**Documentation is part of the definition of done.** A PR that changes public behavior without touching docs is incomplete.

---

## Security posture

State this plainly and repeatedly, including in the app UI:

- **Unaudited. Testnet only. Do not deploy to mainnet or handle real value until an external audit is complete.**
- No mainnet addresses published until then.
- `SECURITY.md` with a private disclosure path. Do not accept vulnerability reports through public issues.
- Run `cargo audit` in CI.
- Consider running CoinFabrik's Scout, which is an open-source static analyzer built for Soroban, as a CI step.
- When ready, the Stellar bug bounty covers Soroban platform exploits — worth reading the terms for how they scope contract-level findings.

The credibility of this project rests on not overstating its maturity. A library that says clearly what it has not yet proven is more trustworthy than one that stays quiet.

---

## Roadmap

**v0.1 — foundations.** All six contract crates with full tests. ✅ Contracts done. ✅ Deployed to testnet (see `DEPLOYMENTS.md`). Contract-spec release still outstanding.

**v0.2 — the SDK.** SDK clients for all five contracts, error decoding, Freighter and keypair signers, runnable examples. `MAX_RECIPIENTS` re-measured against testnet. The one-position-per-instance question settled.

**v0.3 — reference app.** Indexer, history, all five primitives in the UI. Docs site live.

**v0.4 — hardening.** Fuzz testing, gas/resource benchmarking, external audit engagement, mainnet readiness review.

Deliberately excluded: multi-chain support, a token issuance module, fiat integration, a hosted service. If these come up in issues, decline them and say why.

---

## Verify before you build

This spec was written from a snapshot and the Soroban toolchain moves quickly. Confirm the following against current sources rather than trusting anything here. Items marked ✅ were verified on 2026-09-07 while building `contracts`.

1. ✅ **Current `soroban-sdk` version** — 27.0.6 stable; 28.0.0-rc.1 is a prerelease and `cargo info` surfaces it first. Rust 1.97.1 satisfies its 1.91 minimum.
2. ✅ **Current `stellar` CLI syntax** — 27.1.0. The CLI was renamed from `soroban` and command shapes have changed across releases; older tutorials will be wrong.
3. **Protocol version on testnet** and which host functions are available. Not yet checked — needs network access.
4. ✅ **Current `@stellar/stellar-sdk` major version** — 17.0.1.
5. ⚠️ **Resource limits** — instruction count, ledger entry size, transaction size. Measured _locally_ at 40 recipients for `batch_payout`; the local environment does not model transaction size, so this must be re-measured against testnet.
6. **Whether the Stellar Asset Contract interface has changed** for the token calls the contracts make. `transfer`, `transfer_from` and `approve` all work as expected against soroban-sdk 27's test SAC, but this has not been checked against a live network.

Also note, discovered while building: **`env.events().publish` is deprecated** in soroban-sdk 27 in favour of `#[contractevent]`. Any guidance written against the older API will produce deprecation warnings that CI treats as errors.

Where current documentation contradicts this spec, current documentation wins. Note the discrepancy in the PR description so the spec can be corrected.

---

## Contributing

Issues labelled `good-first-issue` are scoped so that someone new to Soroban can complete them. Start there, comment to claim, and open a draft PR early — an in-progress PR with questions is more useful than a perfect one that arrives three weeks late.

If you are unsure whether something is in scope, open an issue before writing code.

## Updates

Updated documentation for recent changes.
