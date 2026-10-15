//! Recurring error surface.
//!
//! Errors are shared org-wide -- see `sororail_common::errors`, where recurring
//! owns the numeric range **80–99**. This module re-exports the enum and
//! documents which variants this contract can return:
//!
//! | Variant | Raised by |
//! |---|---|
//! | [`Error::AlreadyInitialized`] | `authorize` on an initialized instance |
//! | [`Error::NotInitialized`] | any entry point before `authorize` |
//! | [`Error::Unauthorized`] | `cancel` by someone who is neither party |
//! | [`Error::InvalidAmount`] | non-positive `amount_per_period`, or `max_periods == Some(0)` |
//! | [`Error::InvalidDuration`] | zero `period_seconds` |
//! | [`Error::InvalidTimeRange`] | `authorize` or `charge` scheduling the next charge past `u64::MAX` |
//! | [`Error::Overflow`] | `periods_charged` exceeding `u32` |
//! | [`Error::RecurringCancelled`] | `charge` or `cancel` after cancellation |
//! | [`Error::RecurringPeriodNotElapsed`] | `charge` before `next_chargeable_at` |
//! | [`Error::RecurringExhausted`] | `charge` once `max_periods` is reached |
//!
//! # What is absent, and why
//!
//! `Unauthorized` **does** appear here, unlike in `stream` and `vesting`.
//! `cancel` may be called by either of two parties, so it takes a `caller`
//! argument and checks it against the `{payer, payee}` allow-list before
//! `require_auth`; a third party fails that membership check. `charge` acts on
//! a single fixed party (the payee) and so calls `require_auth` directly --
//! a wrong caller there fails at authorization, not with `Unauthorized`.
//!
//! The following shared variants are never returned by this contract:
//!
//! - `RecurringNotFound` (80) -- each instance holds exactly one
//!   authorization, so a missing record is reported as `NotInitialized`. The
//!   number is reserved for an id-keyed design (see "Open design question" in
//!   SPEC.md).
//! - `InsufficientBalance` -- the contract holds no funds. A payer whose
//!   balance or allowance is too small makes the token's `transfer_from` fail
//!   with the token's own error, which aborts the whole `charge`.
//! - `DeadlineNotReached` / `DeadlinePassed` -- there is no deadline; timing is
//!   expressed as `next_chargeable_at` and reported as
//!   `RecurringPeriodNotElapsed`.
//! - `InvalidState` -- the lifecycle is a single `cancelled` flag plus the
//!   `max_periods` cap, each with its own specific variant
//!   (`RecurringCancelled`, `RecurringExhausted`).
//! - `IdenticalParties` -- `authorize` does not reject `payer == payee`. A
//!   self-subscription moves no value, so it is pointless rather than unsafe.
//! - `Underflow`, `DivisionByZero`, `InvalidBasisPoints` -- no subtraction,
//!   division or basis-point math is performed on balances.

pub use sororail_common::Error;
