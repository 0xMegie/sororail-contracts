//! Stream error surface.
//!
//! Errors are shared org-wide -- see `sororail_common::errors`, where stream
//! owns the numeric range **40–59**. This module re-exports the enum and
//! documents which variants this contract can return:
//!
//! | Variant | Raised by |
//! |---|---|
//! | [`Error::AlreadyInitialized`] | `create` on an initialized instance |
//! | [`Error::NotInitialized`] | any entry point before `create` |
//! | [`Error::IdenticalParties`] | `create` with `sender == recipient` |
//! | [`Error::InvalidAmount`] | non-positive rate or withdrawal; `top_up` not a multiple of the rate |
//! | [`Error::InvalidTimeRange`] | `create` with `stop <= start`; `top_up` overflowing `stop` |
//! | [`Error::Overflow`] | funding or accrual exceeding `i128` |
//! | [`Error::StreamCancelled`] | any mutation after cancellation |
//! | [`Error::StreamNotCancellable`] | `cancel` on a stream created with `cancellable = false` |
//! | [`Error::StreamInsufficientAccrued`] | `withdraw` for more than has accrued |
//! | [`Error::StreamNotExtendable`] | `extend` with `new_stop` not after the current stop |
//!
//! # What is absent, and why
//!
//! `Unauthorized` does not appear. Stream's privileged entry points act on a
//! single fixed party each -- the sender for `cancel`, `top_up` and `extend`,
//! the recipient for `withdraw` -- so they call `require_auth` directly on that
//! party rather than checking a caller argument against an allow-list. A wrong
//! caller fails at authorization, not at a membership check.
//!
//! The following shared variants are never returned by this contract either:
//!
//! - `StreamNotFound` (40) -- each instance holds exactly one stream, so a
//!   missing record is reported as `NotInitialized`. The number is reserved for
//!   an id-keyed design (see "Open design question" in SPEC.md).
//! - `InvalidState` -- the only lifecycle transition is cancellation, which has
//!   its own specific variants (`StreamCancelled`, `StreamNotCancellable`).
//! - `InsufficientBalance` -- the stream is fully funded up front, so it
//!   always holds what it owes. Over-withdrawal is the more specific
//!   `StreamInsufficientAccrued`; a sender who cannot fund `create`, `top_up`
//!   or `extend` makes the token's `transfer` fail with the token's own error.
//! - `DeadlineNotReached` / `DeadlinePassed` -- `stop` is an accrual bound,
//!   not a deadline; nothing is gated on it having passed.
//! - `InvalidDuration` -- time is expressed as `start`/`stop` timestamps, so a
//!   bad span is `InvalidTimeRange`.
//! - `InvalidBasisPoints` -- no basis-point math.
//! - `Underflow`, `DivisionByZero` -- every subtraction is bounded by the
//!   `withdrawn + refunded + remaining == deposited` invariant, and the only
//!   division is by `rate_per_second`, which `create` requires to be positive.

pub use sororail_common::Error;
