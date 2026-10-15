# Contributing

Thanks for considering a contribution. This repository holds contracts that are
intended to move money, so the bar for merging is deliberately high — but the
path in is meant to be clear.

## Before you start

Issues labelled `good-first-issue` are scoped so that someone new to Soroban can
complete them: each states the problem, the affected file paths, the expected
behavior, acceptance criteria, and how to test the change locally. Comment to
claim one, then open a **draft PR early**. An in-progress PR with questions is
more useful than a perfect one that arrives three weeks late.

If you are unsure whether something is in scope, open an issue before writing
code. Scope discipline is the point of this project — see the non-goals in
[SPEC.md](SPEC.md).

## Setup

```bash
rustup target add wasm32v1-none
cargo install cargo-nextest
make ci          # fmt-check, lint, test, build
```

The toolchain is pinned in `rust-toolchain.toml`; rustup will honour it
automatically.

## What a complete PR looks like

- [ ] Tests added, including an authorization test for any new privileged entry
      point and edge cases for any new arithmetic
- [ ] `make ci` green locally
- [ ] Docs updated — a PR that changes public behavior without touching docs is
      incomplete
- [ ] No unrelated changes
- [ ] Breaking changes called out explicitly in the description

## Rules that will fail review

**Never renumber or remove a released `Error` variant.** The integers are public
ABI; clients decode failures by number. Append inside the contract's documented
range in `contracts/common/src/errors.rs` and leave retired numbers burned.

**No raw arithmetic on balances.** Use `sororail_common::math`. `a + b` on an
`i128` balance will be asked to change even where it happens to be safe, because
the invariant is easier to hold when it has no exceptions.

**Check membership before `require_auth`.** Use the guards in
`sororail_common::auth`. Calling `require_auth` first prompts an address that
was never permitted to act.

**Extend the TTL of anything you touch.** Soroban archives state that is not
extended. Use `sororail_common::storage`.

**Illegal state transitions must error, not panic.** A panic gives the caller a
useless failure; a typed error tells them what happened.

## Entry Point Anatomy

New contract entry points should follow the same order every time so review can focus on behavior instead of rediscovering authorization and state assumptions:

1. Load the minimal state needed to decide whether the caller is a member of the operation.
2. Check membership or role eligibility before prompting a signature.
3. Call `require_auth` only for an address that is already allowed to act.
4. Check operation-specific flags such as `cancellable` or `revocable` after the caller has been established as an eligible party.
5. Perform arithmetic through `sororail_common::math` and write state through the common storage helpers so TTL handling stays consistent.
6. Return typed errors for rejected state transitions instead of panicking.

This keeps stream, vesting, and escrow entry points aligned: membership first, authentication second, state-specific business rules third, and mutation last.

## Choosing an authorization pattern

Reading the `contract.rs` files side by side, you will see two calling
conventions for what is conceptually the same "is this the right party?"
check. Both are deliberate. Which one an entry point uses depends on a single
question: **how many parties may call it?**

**Exactly one fixed party → no `caller` argument; call `require_auth` on the
stored field.**

```rust
pub fn cancel(env: Env) -> Result<(), Error> {
    let mut stream = storage::load(&env)?;
    // ...state checks...
    stream.sender.require_auth();
```

There is nothing to choose between, so a `caller` argument would only be a
second copy of an address the contract already knows, and one that has to be
checked against it. A wrong caller fails at authorization. `Unauthorized` is
never returned, which is why it is missing from the error tables in `stream`,
`vesting` and `batch_payout`.

Used by `stream` (`withdraw`, `cancel`, `top_up`, `extend`), `vesting`
(`claim`, `revoke`), `recurring::charge`, `escrow` (`init`, `fund`, `cancel`,
`resolve`) and `batch_payout` (`execute`, `execute_equal`).

**More than one permitted party → take `caller: Address`, check membership
with a `sororail_common::auth` guard, which then calls `require_auth`.**

```rust
pub fn release(env: Env, caller: Address) -> Result<(), Error> {
    let mut escrow = storage::load(&env)?;
    // ...state checks...
    auth::require_auth_either_opt(&caller, &escrow.depositor, &escrow.arbiter)?;
```

With several eligible signers the contract cannot know which one is acting, so
the caller has to say, and it needs to be recorded for the event
(`released_by`, `cancelled_by`, `raised_by`). The guard rejects an outsider
with `Unauthorized` *before* any signature is requested, which is the
membership-before-auth rule above.

Used by `escrow` (`release`, `refund`, `dispute`) and `recurring::cancel`.

Rules of thumb:

- Do not add a `caller` argument to a single-party entry point "for
  consistency". It widens the ABI, adds a check that can only ever compare an
  address with itself, and makes `Unauthorized` reachable for no benefit.
- When a single-party entry point gains a second permitted party, switch it to
  the `caller` pattern, use the matching `auth::require_auth_*` guard, and add
  `Unauthorized` to that contract's error table in its `errors.rs`. This is an
  ABI change: call it out as breaking.
- Never call `caller.require_auth()` on an unchecked `caller`. That proves the
  caller signed, not that it is allowed to act.
- Either way, add an authorization test proving a wrong party is rejected.
## Conventions

- **Commits:** [Conventional Commits](https://www.conventionalcommits.org),
  enforced in CI.
- **Branching:** trunk-based. Short-lived branches, PRs into `main`, squash
  merge, linear history.
- **CI must be green to merge.** No exceptions for maintainers.
- **Formatting:** `cargo fmt`. Lints: `cargo clippy`, warnings denied.

## Security

Do not report vulnerabilities through public issues. See [SECURITY.md](SECURITY.md).