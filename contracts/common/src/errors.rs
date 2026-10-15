use soroban_sdk::contracterror;

/// The single error enum shared by every SoroRail contract.
///
/// # Stability contract
///
/// These numbers are part of the public ABI. A client decodes an on-chain
/// failure by its integer, so **a released variant is never renumbered and
/// never removed**. Deprecate instead, and leave the number burned.
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
    InvalidState = 10,
    /// The deadline has not yet been reached.
    DeadlineNotReached = 11,
    /// The deadline has already passed.
    DeadlinePassed = 12,
    /// Not enough balance to satisfy the request.
    InsufficientBalance = 13,
    /// A duration was zero where a positive span is required.
    InvalidDuration = 14,

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

    // ----- stream: 40–59 -----
    /// No stream exists for the given id.
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
    /// No grant exists for the given id.
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
    /// No authorization exists for the given id.
    RecurringNotFound = 80,
    /// The authorization has been cancelled.
    RecurringCancelled = 81,
    /// The current period has already been charged.
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
