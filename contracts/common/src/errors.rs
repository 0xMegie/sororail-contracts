use soroban_sdk::contracterror;

/// The single error enum shared by every SoroRail contract.
///
/// # Stability contract
///
/// These numbers are part of the public ABI. A client decodes an on-chain
/// failure by its integer, so **a released variant is never renumbered and
/// never removed**. Deprecate instead, and leave the number burned. The
/// `error_discriminants_match_the_published_abi_table` test in this crate
/// pins every discriminant so an accidental renumber fails CI.
///
/// New variants are appended inside the owning range. The ranges are:
///
/// | Range     | Owner          |
/// |-----------|----------------|
/// | 1–19      | generic        |
/// | 20–39     | `escrow`       |
/// | 40–59     | `stream`       |
/// | 60–79     | `vesting`      |
/// | 80–99     | `recurring`    |
/// | 100–119   | `batch_payout` |
///
/// Ranges are deliberately sparse so a contract can grow without colliding.
///
/// Not every contract returns every variant. Each contract's own `errors.rs`
/// carries a table of the variants it can raise and a "What is absent, and
/// why" section naming the shared variants it never returns -- keep both in
/// step when adding an error path to a contract.
///
/// # Generic errors that are currently single-contract
///
/// Some variants in the generic range (1–19) are currently used by only one
/// contract but remain generic to allow future reuse without ABI breakage:
///
/// - **`DeadlineNotReached` (11)** and **`DeadlinePassed` (12)**: Currently
///   only `escrow` uses these, but any future time-locked contract (e.g., a
///   vested airdrop, a time-based auction) would naturally reuse them. Keeping
///   them generic avoids having to add `EscrowDeadlineNotReached` now and a
///   separate `VestingDeadlineNotReached` later, fragmenting what is
///   conceptually the same failure mode.
///
/// - **`InvalidState` (10)** and **`InsufficientBalance` (13)**: Reserved for
///   lifecycle and balance checks that contracts currently handle with
///   contract-specific variants (e.g., `EscrowNotFunded`, `StreamCancelled`).
///   Every contract prefers a more specific error today, but these remain
///   available if a future contract's design benefits from a generic state or
///   balance check without needing a new numbered variant.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    // ----- generic: 1–19 -----
    /// `init` called on an instance that already holds config.
    AlreadyInitialized = 1,
    /// An entry point was called before `init`.
    NotInitialized = 2,
    /// The caller is not permitted to perform this action.
    Unauthorized = 3,
    /// An amount was zero or negative where a positive value is required.
    InvalidAmount = 4,
    /// A time range was empty or inverted (e.g. `stop <= start`).
    InvalidTimeRange = 5,
    /// Checked arithmetic exceeded `i128::MAX`.
    Overflow = 6,
    /// Checked arithmetic fell below `i128::MIN`.
    Underflow = 7,
    /// Division by zero.
    DivisionByZero = 8,
    /// Basis points outside the inclusive range 0..=10000.
    InvalidBasisPoints = 9,
    /// The action is not legal from the current state.
    ///
    /// Reserved for generic state checks. Currently unused; contracts prefer
    /// contract-specific variants like `EscrowNotFunded` or `StreamCancelled`.
    /// See module docs for rationale.
    InvalidState = 10,
    /// The deadline has not yet been reached.
    ///
    /// Currently used by `escrow::refund`. Kept generic for future time-locked
    /// contracts. See module docs for rationale.
    DeadlineNotReached = 11,
    /// The deadline has already passed.
    ///
    /// Currently used by `escrow::fund`. Kept generic for future time-locked
    /// contracts. See module docs for rationale.
    DeadlinePassed = 12,
    /// Not enough balance to satisfy the request.
    ///
    /// Reserved for generic balance checks. Currently unused; contracts let
    /// token transfers fail with the token's own error or use contract-specific
    /// variants like `StreamInsufficientAccrued`. See module docs for rationale.
    InsufficientBalance = 13,
    /// A duration was zero where a positive span is required.
    InvalidDuration = 14,
    /// The two counterparties of a transfer must differ.
    IdenticalParties = 15,

    // ----- escrow: 20–39 -----
    /// `fund` called on an escrow that is not in `Created`.
    EscrowNotFundable = 20,
    /// `release` or `refund` called on an escrow that is not in `Funded`.
    EscrowNotFunded = 21,
    /// The escrow has already reached a terminal state.
    EscrowClosed = 22,
    /// The action requires an arbiter and none was configured.
    EscrowNoArbiter = 23,
    /// `resolve` called on an escrow that is not in `Disputed`.
    EscrowNotDisputed = 24,
    /// `dispute` called on an escrow that is already disputed.
    EscrowAlreadyDisputed = 25,
    /// `cancel` called on an escrow that is not in `Created`.
    EscrowNotCancellable = 26,

    // ----- stream: 40–59 -----
    /// Reserved for the future id-keyed stream design.
    StreamNotFound = 40,
    /// The stream has already been cancelled.
    StreamCancelled = 41,
    /// `cancel` called on a stream created with `cancellable = false`.
    StreamNotCancellable = 42,
    /// Withdrawal requested exceeds the accrued, unwithdrawn balance.
    StreamInsufficientAccrued = 43,
    /// `extend` given a `new_stop` that is not after the current stop.
    StreamNotExtendable = 44,

    // ----- vesting: 60–79 -----
    /// Reserved for the future id-keyed vesting design.
    VestingNotFound = 60,
    /// Nothing is claimable yet -- the cliff has not been reached.
    VestingCliffNotReached = 61,
    /// `revoke` called on a grant created with `revocable = false`.
    VestingNotRevocable = 62,
    /// The grant has already been revoked.
    VestingRevoked = 63,
    /// The cliff falls after the end of the vesting period.
    VestingCliffAfterEnd = 64,
    /// Nothing vested-but-unclaimed remains to claim.
    VestingNothingToClaim = 65,
    /// `start + duration` would overflow `u64`.
    VestingScheduleOverflow = 66,

    // ----- recurring: 80–99 -----
    /// Reserved for the future id-keyed recurring design.
    RecurringNotFound = 80,
    /// The authorization has been cancelled.
    RecurringCancelled = 81,
    /// `charge` called before `next_chargeable_at` (including the first charge).
    RecurringPeriodNotElapsed = 82,
    /// `max_periods` has been exhausted.
    RecurringExhausted = 83,

    // ----- batch_payout: 100–119 -----
    /// The recipient list was empty.
    BatchEmpty = 100,
    /// The recipient list exceeds the documented maximum.
    BatchTooLarge = 101,
    // 102 was `BatchDuplicateRecipient`, removed before any release.
    // Detecting duplicates on-chain costs a quadratic scan of the recipient
    // list, and paying one address twice in a batch is legitimate anyway, so
    // duplicate detection belongs in the client's CSV import. The number stays
    // burned rather than reused.
}
