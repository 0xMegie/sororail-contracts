use soroban_sdk::{contracttype, Address};

/// A recurring pull authorization. One per deployed contract instance.
///
/// # How the pull works
///
/// This contract holds no funds. The payer authorizes it as a **spender** on
/// the token (`approve`), and `charge` then calls `transfer_from` to move
/// `amount_per_period` straight from the payer to the payee. The payer's
/// allowance is the real cap, and revoking that allowance on the token stops
/// charges even without touching this contract.
///
/// # Periods never accrue retroactively
///
/// `next_chargeable_at` is set to `now + period_seconds` at each charge --
/// **not** advanced by one period from its previous value. A payee who forgets
/// to charge for three months therefore cannot then charge three times; the
/// skipped periods are simply gone.
///
/// This is a deliberate consumer-protection choice, not an oversight. It costs
/// the payee the revenue of periods they did not collect, and it guarantees
/// the payer can never be surprised by a bundled back-charge.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Authorization {
    /// The subscriber, whose funds are pulled.
    pub payer: Address,
    /// The merchant, who initiates each charge.
    pub payee: Address,
    /// The token being charged.
    pub token: Address,
    /// The most that may be pulled in any one period.
    pub amount_per_period: i128,
    /// The cadence, in seconds.
    pub period_seconds: u64,
    /// Cap on the number of charges, if any. `None` is open-ended.
    pub max_periods: Option<u32>,
    /// How many charges have been taken.
    pub periods_charged: u32,
    /// The earliest ledger timestamp at which the next charge may be taken.
    pub next_chargeable_at: u64,
    /// Whether either party has cancelled.
    ///
    /// A plain flag rather than a `State` enum or a timestamp: the lifecycle
    /// is binary and nothing is computed from when it ended. See "Lifecycle
    /// modeling" in SPEC.md.
    pub cancelled: bool,
}

impl Authorization {
    /// Whether the cap on charges has been reached.
    pub fn is_exhausted(&self) -> bool {
        match self.max_periods {
            Some(max) => self.periods_charged >= max,
            None => false,
        }
    }

    /// Whether a charge may be taken at `at`.
    pub fn is_chargeable_at(&self, at: u64) -> bool {
        !self.cancelled && !self.is_exhausted() && at >= self.next_chargeable_at
    }

    /// Charges still permitted under the cap, if there is one.
    pub fn remaining_periods(&self) -> Option<u32> {
        self.max_periods
            .map(|max| max.saturating_sub(self.periods_charged))
    }
}
