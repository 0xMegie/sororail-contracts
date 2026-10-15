//! Escrow error surface.
//!
//! Errors are not defined per crate. Every SoroRail contract shares the single
//! [`Error`] enum in `sororail_common::errors`, which assigns each contract a
//! documented numeric range so that one integer never means two things across
//! the org. Escrow owns **20–39**.
//!
//! This module re-exports the shared enum so callers can write
//! `sororail_escrow::Error`, and documents which variants this contract can
//! actually return:
//!
//! | Variant | Raised by |
//! |---|---|
//! | [`Error::AlreadyInitialized`] | `init` on an initialized instance |
//! | [`Error::NotInitialized`] | any entry point before `init` |
//! | [`Error::Unauthorized`] | a caller outside the permitted set |
//! | [`Error::InvalidAmount`] | `init` with a non-positive amount |
//! | [`Error::InvalidTimeRange`] | `init` with a deadline at or before now |
//! | [`Error::InvalidBasisPoints`] | `resolve` with `split_bps > 10000` |
//! | [`Error::Overflow`] | `resolve` on an amount too large to scale by `split_bps` in `i128` |
//! | [`Error::DeadlineNotReached`] | depositor `refund` before the deadline; `refund` of a disputed escrow still inside its grace period |
//! | [`Error::DeadlinePassed`] | `fund` at or after the deadline |
//! | [`Error::EscrowNotFundable`] | `fund` when not in `Created` |
//! | [`Error::EscrowNotFunded`] | `release`/`refund`/`dispute` when not in `Funded` |
//! | [`Error::EscrowClosed`] | any transition from a terminal state |
//! | [`Error::EscrowNoArbiter`] | `dispute` with no arbiter configured (`resolve` is unreachable — see invariant in `contract.rs`) |
//! | [`Error::EscrowNotDisputed`] | `resolve` when not in `Disputed` |
//! | [`Error::EscrowAlreadyDisputed`] | `dispute` when already disputed |
//! | [`Error::EscrowNotCancellable`] | `cancel` when not in `Created` |
//!
//! # What is absent, and why
//!
//! `Unauthorized` **does** appear here, unlike in `stream` and `vesting`.
//! `release`, `refund` and `dispute` may each be called by more than one party,
//! so they take a `caller` argument and check it against an allow-list before
//! `require_auth`. `init`, `fund`, `cancel` and `resolve` act on a single fixed
//! party (the depositor or the arbiter) and call `require_auth` directly, so a
//! wrong caller there fails at authorization rather than with `Unauthorized`.
//!
//! The following shared variants are never returned by this contract:
//!
//! - `InvalidState` -- escrow has an explicit [`crate::types::State`] machine,
//!   and every illegal transition reports the specific escrow variant that
//!   names it (`EscrowNotFundable`, `EscrowNotFunded`, `EscrowClosed`,
//!   `EscrowNotDisputed`, `EscrowAlreadyDisputed`, `EscrowNotCancellable`)
//!   instead of the generic one.
//! - `InsufficientBalance` -- the contract never checks balances itself. A
//!   depositor who cannot cover `amount` makes the token's `transfer` fail with
//!   the token's own error, which aborts `fund`; once funded, the contract
//!   always holds exactly `amount`.
//! - `InvalidDuration` -- the deadline is an absolute timestamp, not a span, so
//!   a bad deadline is reported as `InvalidTimeRange`.
//! - `IdenticalParties` -- `init` does not reject `depositor == beneficiary`.
//!   Such an escrow can only return funds to the address that supplied them.
//! - `Underflow`, `DivisionByZero` -- `split_bps` derives the remainder by
//!   subtraction from a smaller share and divides by the constant `10000`, so
//!   neither can occur.

pub use sororail_common::Error;
