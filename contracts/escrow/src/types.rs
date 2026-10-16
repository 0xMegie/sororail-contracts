use soroban_sdk::{contracttype, Address};

/// The escrow lifecycle.
///
/// ```text
/// Created ──fund──▶ Funded ──release──▶ Released
///                     │
///                     ├────refund────▶ Refunded
///                     │
///                     └───dispute────▶ Disputed ──resolve──▶ Resolved
/// ```
///
/// `Released`, `Refunded` and `Resolved` are terminal. Every other transition
/// is illegal and returns an error rather than panicking.
///
/// Escrow is the only contract with an explicit state enum because it is the
/// only one whose lifecycle branches: `Funded` has three exits and `Disputed`
/// is a second non-terminal state. The binary lifecycles in `stream`,
/// `vesting` and `recurring` use an optional timestamp or flag instead. See
/// "Lifecycle modeling" in SPEC.md for when to choose which.
///
/// Discriminants are part of the ABI; do not renumber them.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum State {
    /// Configured, but the depositor has not transferred the funds yet.
    ///
    /// Reachable only from `init`. Transitions to `Funded` via `fund`, or to
    /// `Refunded` via `cancel` (the depositor backing out before funding).
    Created = 0,
    /// Holding the funds, awaiting release, refund or dispute.
    ///
    /// Reached from `Created` via `fund`. Transitions to `Released` via
    /// `release`, to `Refunded` via `refund`, or to `Disputed` via `dispute`
    /// (only when an arbiter is configured).
    Funded = 1,
    /// Paid out to the beneficiary. Terminal.
    ///
    /// Reached from `Funded` via `release`.
    Released = 2,
    /// Returned to the depositor. Terminal.
    ///
    /// Reached from `Created` via `cancel`, from `Funded` via `refund`, or
    /// from `Disputed` via `refund` (arbiter grace-period recovery).
    Refunded = 3,
    /// Frozen pending an arbiter decision.
    ///
    /// Reached from `Funded` via `dispute`. Transitions to `Resolved` via
    /// `resolve`, or to `Refunded` via `refund` once the dispute grace
    /// period has elapsed with no arbiter action.
    Disputed = 4,
    /// Split between the parties by the arbiter. Terminal.
    ///
    /// Reached from `Disputed` via `resolve`.
    Resolved = 5,
}

impl State {
    /// Whether no further transition is possible.
    pub fn is_terminal(self) -> bool {
        matches!(self, State::Released | State::Refunded | State::Resolved)
    }
}

/// The full escrow agreement. One per deployed contract instance.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Escrow {
    /// Funds this escrow and receives a refund.
    pub depositor: Address,
    /// Receives the funds on release.
    pub beneficiary: Address,
    /// May release, refund at any time, and resolve disputes. Optional --
    /// without one, `dispute` and `resolve` are unavailable.
    pub arbiter: Option<Address>,
    /// The token being escrowed.
    pub token: Address,
    /// The amount held, in the token's smallest unit.
    pub amount: i128,
    /// Ledger timestamp after which the depositor may unilaterally refund.
    pub deadline: u64,
    /// Current lifecycle position.
    pub state: State,
}
