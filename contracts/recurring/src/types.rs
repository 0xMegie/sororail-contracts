use soroban_sdk::{contracttype, Address};
use sororail_common::math;

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
///
/// # The pull invariant
///
/// Because this contract holds no funds, there is nothing for it to conserve
/// internally; the invariant is on what it can move. Every successful `charge`
/// transfers exactly `amount_per_period` from the payer to the payee and
/// increments `periods_charged` by one, and a failed transfer reverts the
/// increment with it. So the total ever pulled is always exactly:
///
/// ```text
/// periods_charged * amount_per_period
/// ```
///
/// and, when `max_periods` is set, never more than `max_periods *
/// amount_per_period`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Authorization {
    /// The subscriber, whose funds are pulled.
    pub payer: Address,
    /// The merchant, who initiates each charge.
    pub payee: Address,
    /// The token being charged.
    pub token: Address,
    /// The most that may be pulled in any one period, in the token's smallest
    /// unit (e.g., stroops for XLM, or the base unit defined by the token's
    /// decimals).
    pub amount_per_period: i128,
    /// The cadence, in seconds. Each charge advances `next_chargeable_at` by
    /// this duration.
    pub period_seconds: u64,
    /// Cap on the number of charges, if any. `None` means no cap: charges may
    /// continue indefinitely as long as the authorization is not cancelled and
    /// the payer's token allowance remains sufficient.
    pub max_periods: Option<u32>,
    /// How many charges have been taken.
    pub periods_charged: u32,
    /// The earliest ledger timestamp at which the next charge may be taken, in
    /// seconds since the Unix epoch.
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
        self.remaining_periods() == Some(0)
    }

    /// Whether a charge may be taken at `at`.
    pub fn is_chargeable_at(&self, at: u64) -> bool {
        !self.cancelled && !self.is_exhausted() && at >= self.next_chargeable_at
    }

    /// Charges still permitted under the cap, if there is one.
    pub fn remaining_periods(&self) -> Option<u32> {
        math::remaining_count(self.max_periods, self.periods_charged)
    }
}

#[cfg(test)]
mod tests {
    use soroban_sdk::{testutils::Address as _, Address, Env};

    use super::Authorization;

    #[test]
    fn cap_checks_match_remaining_period_count() {
        let env = Env::default();
        let authorization = |max_periods, periods_charged| Authorization {
            payer: Address::generate(&env),
            payee: Address::generate(&env),
            token: Address::generate(&env),
            amount_per_period: 1,
            period_seconds: 1,
            max_periods,
            periods_charged,
            next_chargeable_at: 0,
            cancelled: false,
        };

        let below = authorization(Some(3), 2);
        assert_eq!(below.remaining_periods(), Some(1));
        assert!(!below.is_exhausted());

        let at_cap = authorization(Some(3), 3);
        assert_eq!(at_cap.remaining_periods(), Some(0));
        assert!(at_cap.is_exhausted());

        let above_cap = authorization(Some(3), 4);
        assert_eq!(above_cap.remaining_periods(), Some(0));
        assert!(above_cap.is_exhausted());

        let uncapped = authorization(None, u32::MAX);
        assert_eq!(uncapped.remaining_periods(), None);
        assert!(!uncapped.is_exhausted());
    }
}
