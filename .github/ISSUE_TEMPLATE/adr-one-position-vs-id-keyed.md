---
name: "ADR: one-position-per-instance vs. id-keyed contracts"
about: >
  Architectural decision required before the SDK is written. Track the discussion
  and record the chosen design here. See SPEC.md § "Open design question" and
  DEPLOYMENTS.md § "These are single-use instances".
title: "ADR: settle one-position-per-instance vs. id-keyed storage before SDK work begins"
labels: ["contract", "size/l"]
assignees: []
---

## Context

Every contract except `batch_payout` currently holds **one position per
deployed instance** — one escrow, one stream, one grant, one subscription.
Entry-point signatures take no position id; the first caller to `init` /
`create` / `authorize` claims the deployment permanently.

This was flagged as an open question in
[SPEC.md § "Open design question"](../SPEC.md#open-design-question) and
in [DEPLOYMENTS.md § "These are single-use instances"](../DEPLOYMENTS.md#these-are-single-use-instances)
with the explicit note: **settle it before the SDK is written**.

The `frontend` repo's SDK design depends directly on this choice:

- A one-position model means one contract address = one stream/escrow/grant.
  The SDK must manage a registry of addresses; the app must track them in its
  database.
- An id-keyed model means one contract address = many positions, looked up by
  a position id. The SDK method signatures change (every call takes an id), the
  storage layout changes, and the on-chain resource profile per call changes.

Changing the storage layout after the SDK ships is an ABI break and a
migration problem for anyone already holding positions on testnet.

## Decision required

Choose **one** of:

**Option A — keep one-position-per-instance.**
No contract changes. The SDK wraps one address per position, holds a registry,
and the app stores addresses. Simple contracts, more complex SDK/app bookkeeping.

**Option B — migrate to id-keyed storage.**
Each contract gets a `position_id: u64` (or `Bytes`/`Symbol`) in every entry
point. Storage keys are namespaced by id. One deployed instance holds many
positions. More complex contracts, simpler SDK/app address management, better
fit for the payroll and dashboard screens described in SPEC.md.

**Option C — hybrid: id-keyed for `stream`, `vesting`, `recurring`; keep
`escrow` and `batch_payout` as-is.**
`batch_payout` is already stateless. `escrow` is naturally one-off and
negotiated between two specific parties, so a new deploy per escrow is
defensible. The high-volume primitives (streams, subscriptions, vesting
schedules) benefit most from id-keying.

## Acceptance criteria for this issue

- [ ] The decision is recorded as a comment on this issue, with rationale.
- [ ] SPEC.md § "Open design question" is updated to reflect the chosen design
      and this issue is linked.
- [ ] DEPLOYMENTS.md § "These are single-use instances" is updated accordingly.
- [ ] If Option B or C: a follow-up issue is opened for the contract migration,
      with a migration test that exercises the new storage layout.
- [ ] The `frontend` repo is notified (link this issue in the SDK kickoff issue
      there) so SDK work does not start until this is resolved.

## Deadline

This must be resolved before any SDK code is merged. Per SPEC.md build order,
`frontend` is blocked on `contracts` being finished — this is the last open
question in `contracts`.

## References

- [SPEC.md § Open design question](../SPEC.md#open-design-question)
- [DEPLOYMENTS.md § These are single-use instances](../DEPLOYMENTS.md#these-are-single-use-instances)
- [SPEC.md § Build order](../SPEC.md#build-order) — `frontend` depends on `contracts`
- [SPEC.md § packages/sdk](../SPEC.md#packagessdk) — SDK design affected by this choice
