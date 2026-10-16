use soroban_sdk::{contracttype, Address};

/// Maximum recipients permitted in one batch.
///
/// # Where this number comes from
///
/// Measured, not guessed, as SPEC.md requires. `report_batch_ceiling` in
/// `test.rs` ramps batch size until execution fails; against soroban-sdk
/// 27.0.6 on 2026-09-07 it found **40 recipients execute and 45 exceed the
/// budget** (`Error(Budget, ExceededLimit)`). An initial guess of 100 was
/// wrong by more than a factor of two, which is the argument for measuring.
///
/// `max_recipients_is_within_what_actually_executes` re-measures on every CI
/// run and fails if this constant drifts above what the code can actually do,
/// so added logic that raises the per-recipient cost cannot silently make the
/// documented cap a lie.
///
/// # It is still not the on-chain ceiling
///
/// The local test environment models the CPU/memory budget but **not** the
/// transaction size limit a real network applies to the submitted envelope,
/// and its `mock_all_auths` builds a separate authorization entry per transfer
/// where a real submission signs one tree. The real figure could be higher or
/// lower. Re-run the ramp against testnet before `v0.2` and set this from that
/// measurement.
///
/// Erring low is deliberate: a refused batch is an inconvenience, a batch that
/// exhausts the ledger budget is an outage.
pub const MAX_RECIPIENTS: u32 = 40;

/// One line of a batch: who is paid, and how much.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Payment {
    /// The recipient's address. Must be a valid Stellar address.
    pub to: Address,
    /// The amount to transfer, in the token's smallest unit (e.g., stroops for
    /// XLM, or the base unit defined by the token's decimals). Must be
    /// non-negative; negative amounts are rejected.
    pub amount: i128,
}

/// The outcome of a batch, returned to the caller and emitted as an event.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Receipt {
    /// How many recipients were paid.
    pub count: u32,
    /// The sum actually moved.
    pub total: i128,
}

// There was a `pub type Payments = Vec<Payment>` alias here, used in the
// `execute` and `preview` signatures. Do not reintroduce it.
//
// `#[contractimpl]` reads argument types syntactically and cannot resolve an
// alias, so it emitted a contract spec referring to a user-defined type called
// `Payments` that does not exist. The contract deployed and the Rust tests all
// passed, but every client reading the spec broke: `stellar contract invoke`
// failed with `Missing Entry Payments` on *any* function of the contract,
// including argument-less ones, because the whole interface failed to load.
//
// Spell soroban types out in full in entry-point signatures.
