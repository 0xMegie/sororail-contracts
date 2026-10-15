# Error codes

The canonical, documented mapping of `sororail_common::errors::Error` to its
stable ABI numbers. Clients decode on-chain failures by these integers, so a
released row here is never renumbered or removed — see the stability
contract in `errors.rs`.

This table is checked against `contracts/common/src/errors.rs` by
`scripts/check_error_docs.sh` in CI. Add a row here in the same PR that adds
or renumbers a variant in `errors.rs`.

| Code | Variant | Owner | Meaning |
|------|---------|-------|---------|
| 1 | `AlreadyInitialized` | generic | `init` called on an instance that already holds config. |
| 2 | `NotInitialized` | generic | An entry point was called before `init`. |
| 3 | `Unauthorized` | generic | The caller is not permitted to perform this action. |
| 4 | `InvalidAmount` | generic | An amount was zero or negative where a positive value is required. |
| 5 | `InvalidTimeRange` | generic | A time range was empty or inverted (e.g. `stop <= start`). |
| 6 | `Overflow` | generic | Checked arithmetic exceeded `i128::MAX`. |
| 7 | `Underflow` | generic | Checked arithmetic fell below `i128::MIN`. |
| 8 | `DivisionByZero` | generic | Division by zero. |
| 9 | `InvalidBasisPoints` | generic | Basis points outside the inclusive range 0..=10000. |
| 10 | `InvalidState` | generic | The action is not legal from the current state. |
| 11 | `DeadlineNotReached` | generic | The deadline has not yet been reached. |
| 12 | `DeadlinePassed` | generic | The deadline has already passed. |
| 13 | `InsufficientBalance` | generic | Not enough balance to satisfy the request. |
| 14 | `InvalidDuration` | generic | A duration was zero where a positive span is required. |
| 15 | `IdenticalParties` | generic | The two counterparties of a transfer must differ. |
| 20 | `EscrowNotFundable` | escrow | `fund` called on an escrow that is not in `Created`. |
| 21 | `EscrowNotFunded` | escrow | `release` or `refund` called on an escrow that is not in `Funded`. |
| 22 | `EscrowClosed` | escrow | The escrow has already reached a terminal state. |
| 23 | `EscrowNoArbiter` | escrow | The action requires an arbiter and none was configured. |
| 24 | `EscrowNotDisputed` | escrow | `resolve` called on an escrow that is not in `Disputed`. |
| 25 | `EscrowAlreadyDisputed` | escrow | `dispute` called on an escrow that is already disputed. |
| 26 | `EscrowNotCancellable` | escrow | `cancel` called on an escrow that is not in `Created`. |
| 40 | `StreamNotFound` | stream | No stream exists for the given id. |
| 41 | `StreamCancelled` | stream | The stream has already been cancelled. |
| 42 | `StreamNotCancellable` | stream | `cancel` called on a stream created with `cancellable = false`. |
| 43 | `StreamInsufficientAccrued` | stream | Withdrawal requested exceeds the accrued, unwithdrawn balance. |
| 44 | `StreamNotExtendable` | stream | `extend` given a `new_stop` that is not after the current stop. |
| 60 | `VestingNotFound` | vesting | No grant exists for the given id. |
| 61 | `VestingCliffNotReached` | vesting | Nothing is claimable yet — the cliff has not been reached. |
| 62 | `VestingNotRevocable` | vesting | `revoke` called on a grant created with `revocable = false`. |
| 63 | `VestingRevoked` | vesting | The grant has already been revoked. |
| 64 | `VestingCliffAfterEnd` | vesting | The cliff falls after the end of the vesting period. |
| 65 | `VestingNothingToClaim` | vesting | Nothing vested-but-unclaimed remains to claim. |
| 80 | `RecurringNotFound` | recurring | No authorization exists for the given id. |
| 81 | `RecurringCancelled` | recurring | The authorization has been cancelled. |
| 82 | `RecurringPeriodNotElapsed` | recurring | `charge` called before `next_chargeable_at` (including the first charge). |
| 83 | `RecurringExhausted` | recurring | `max_periods` has been exhausted. |
| 100 | `BatchEmpty` | batch_payout | The recipient list was empty. |
| 101 | `BatchTooLarge` | batch_payout | The recipient list exceeds the documented maximum. |

`102` (`BatchDuplicateRecipient`) was removed before any release and stays
burned rather than reused; see the comment at the end of `errors.rs`.
