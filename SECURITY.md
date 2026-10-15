# Security Policy

## Status: unaudited, testnet only

**These contracts have not been audited. Do not deploy them to mainnet and do
not use them to handle real value.** No mainnet addresses will be published
until an external audit is complete.

This is stated here, in the README, and in the reference application's UI. The
credibility of this project rests on not overstating its maturity.

## Maintainers

| Role               | GitHub                                                   | Contact                                 |
| ------------------ | -------------------------------------------------------- | --------------------------------------- |
| Primary maintainer | [@Chidubemkingsley](https://github.com/Chidubemkingsley) | via GitHub private advisory (preferred) |
| Backup escalation  | [@Chidubemkingsley](https://github.com/Chidubemkingsley) | rubelu2017@gmail.com                    |

The primary maintainer is responsible for meeting the response-time commitments
below. If a report receives no acknowledgement within 72 hours, use the backup
contact directly. Both contacts are monitored; the private advisory channel is
preferred because it keeps the disclosure trail in one place.

## Reporting a vulnerability

**Do not open a public issue for a security report.**

Use GitHub's private vulnerability reporting on this repository
(Security → Report a vulnerability), which opens a private advisory visible
only to maintainers.

Please include: the affected contract and version or commit, a description of
the impact, and the smallest reproduction you have — ideally a failing test.

We aim to acknowledge a report within 72 hours and to give an assessment with a
remediation timeline within seven days. We will credit reporters in the
advisory unless asked not to.

## Scope

In scope: the contracts in this repository — incorrect authorization, loss or
lock-up of funds, conservation violations (withdrawn + refunded + remaining not
equalling deposited), arithmetic errors, and state-machine transitions that
should be illegal.

**Note on `batch_payout`:** the conservation-invariant item above applies to
`escrow`, `stream`, `vesting`, and `recurring`, all of which hold deposited
funds on behalf of parties. `batch_payout` is stateless and holds no funds
between calls — it routes a transfer atomically and exits, so there is no
deposited balance and no conservation invariant to violate in the same sense.
Security findings against `batch_payout` are still in scope (incorrect
authorization, arithmetic errors in the fee/amount split, or a malformed
recipients list that causes unexpected behaviour), but conservation-invariant
analysis does not apply to it.

Out of scope: findings that require a compromised wallet or leaked key,
issues in the Stellar network or `soroban-sdk` itself (report those upstream),
and anything that depends on deploying to mainnet, which we ask you not to do.

## Known limitations and design decisions

### Escrow dispute timeout

The `escrow` contract requires an arbiter to resolve disputed escrows by calling
`resolve()`. If the arbiter's key is lost or they become unresponsive, funds
would otherwise be locked permanently in the `Disputed` state. To mitigate this:

- Once the original `deadline` passes, a 7-day grace period begins.
- If a dispute remains unresolved after this grace period, **anyone** may call
  `refund()` to return the full amount to the original depositor, recovering
  funds from an unresponsive arbiter.
- This design prioritizes recovery over absolute arbiter authority. An arbiter
  who cares about their role should resolve disputes well before the deadline.

Integrators relying on escrow should document this assumption: arbiters are
trusted to respond within 7 days of the original deadline, or funds become
recoverable by the depositor unilaterally.

## Public disclosure and security advisories

Once a vulnerability is fixed and the patched release is published, we will
publish a GitHub Security Advisory on this repository so that downstream
integrators can assess their retroactive exposure. The advisory will include:
the affected versions, a description of the impact, the fix commit, and credit
to the reporter (unless they asked to remain anonymous).

Our target timeline from fix-merged to advisory-published is **14 days**. This
window gives integrators who track the repository time to upgrade before the
full technical details are public, while keeping the dark period short enough
that people who missed the patch notice are not left exposed indefinitely.

If coordinated disclosure with a third party (e.g. a downstream project or the
Stellar bug bounty program) requires a longer embargo, we will say so in our
initial response to the reporter and agree on a date. We will not extend an
embargo past **90 days** from the original report without the reporter's
explicit consent.

For reporters: you are welcome to publish your own write-up after the advisory
goes live. Please link to the advisory so readers can verify the fix.

## Automated checks

CI runs `cargo audit` on every pull request and on every push to `main`, via
the `audit` job in `.github/workflows/ci.yml` (`rustsec/audit-check` v2.0.0,
pinned by commit SHA). It scans `Cargo.lock` against the
[RustSec advisory database](https://rustsec.org/advisories/).

**Failure policy.** The action has no severity-threshold setting. It decides
pass/fail by advisory _type_, not by CVSS score:

| Finding                                                                                           | Effect on CI                        |
| ------------------------------------------------------------------------------------------------- | ----------------------------------- |
| Vulnerability advisory, **any severity** (low, medium, high or critical, or no CVSS score at all) | **Fails the build**                 |
| Informational advisory: `unmaintained`, `unsound`, `notice`                                       | Reported as a warning, build passes |
| Yanked crate version                                                                              | Reported as a warning, build passes |

In other words, a dependency with any known vulnerability blocks the PR, even
a low-severity one. Unmaintained, unsound and yanked dependencies are
surfaced in the "Security audit" check output but do not block. Maintainers
should still review those warnings when they appear. PRs from forks follow
the same rule: the action can't publish a check run there, so it prints the
report to the job log and fails the job itself.

No advisories are currently ignored. Ignoring one requires adding its
`RUSTSEC-YYYY-NNNN` ID to the action's `ignore` input in `ci.yml`, together
with a comment explaining why it doesn't affect these contracts, and it must
go through normal review.

Adding CoinFabrik's Scout, an open-source static analyzer built for Soroban,
is tracked as follow-up work.

The Stellar bug bounty covers Soroban platform exploits; contract-level
findings in this repository are ours, not theirs.
