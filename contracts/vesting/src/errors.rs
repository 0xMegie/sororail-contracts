//! Vesting error surface.
//!
//! Errors are shared org-wide -- see `sororail_common::errors`, where vesting
//! owns the numeric range **60–79**. This module re-exports the enum and
//! documents which variants this contract can return:
//!
//! | Variant | Raised by |
//! |---|---|
//! | [`Error::AlreadyInitialized`] | `create` on an initialized instance |
//! | [`Error::NotInitialized`] | any entry point before `create` |
//! | [`Error::IdenticalParties`] | `create` with `grantor == beneficiary` |
//! | [`Error::InvalidAmount`] | `create` with a non-positive total |
//! | [`Error::InvalidDuration`] | `create` with a zero duration |
//! | [`Error::Overflow`] | proportional vesting math exceeding `i128` |
//! | [`Error::VestingCliffAfterEnd`] | `create` with `cliff > duration` |
//! | [`Error::VestingCliffNotReached`] | `claim` before `start + cliff` |
//! | [`Error::VestingNothingToClaim`] | `claim` with nothing vested-but-unclaimed |
//! | [`Error::VestingNotRevocable`] | `revoke` on a grant created with `revocable = false` |
//! | [`Error::VestingRevoked`] | `revoke` on an already-revoked grant |
//!
//! # What is absent, and why
//!
//! As with `stream`, `Unauthorized` does not appear: each privileged entry
//! point acts on one fixed party -- the beneficiary for `claim`, the grantor
//! for `revoke` -- so a wrong caller fails at `require_auth` rather than at a
//! membership check.
//!
//! The following shared variants are never returned by this contract either:
//!
//! - `VestingNotFound` (60) -- each instance holds exactly one grant, so a
//!   missing record is reported as `NotInitialized`. The number is reserved for
//!   an id-keyed design (see "Open design question" in SPEC.md).
//! - `InvalidState` -- the only lifecycle transition is revocation, which has
//!   its own specific variants (`VestingRevoked`, `VestingNotRevocable`).
//! - `InvalidTimeRange` -- `cliff` and `duration` are spans from `start`, not
//!   timestamps, so bad inputs are `InvalidDuration` or `VestingCliffAfterEnd`.
//! - `InsufficientBalance` -- the grant is fully funded at `create`. "Nothing
//!   left to take" is the more specific `VestingNothingToClaim`; a grantor who
//!   cannot fund `create` makes the token's `transfer` fail with the token's
//!   own error.
//! - `DeadlineNotReached` / `DeadlinePassed` -- the one time gate is the cliff,
//!   reported as `VestingCliffNotReached`.
//! - `InvalidBasisPoints` -- no basis-point math.
//! - `Underflow`, `DivisionByZero` -- vested never exceeds `total` and claimed
//!   never exceeds vested, and the only divisor is `duration`, which `create`
//!   requires to be non-zero.

pub use sororail_common::Error;
