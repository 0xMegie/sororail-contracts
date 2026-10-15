use soroban_sdk::{contracttype, Address};
use sororail_common::{math, Error};

/// A vesting grant. One per deployed contract instance.
///
/// # The schedule
///
/// `cliff` and `duration` are both **spans in seconds measured from `start`**,
/// not absolute timestamps. Nothing vests before `start + cliff`; at that
/// moment the elapsed proportion vests in one step, and vesting then continues
/// linearly until fully vested at `start + duration`.
///
/// `cliff == duration` is legal and means a single all-or-nothing unlock.
/// `cliff == 0` means linear vesting from `start` with no cliff.
///
/// # Conservation
///
/// ```text
/// claimed + returned + remaining == total
/// ```
///
/// exactly, at every point, where `remaining` is what the contract still holds.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Grant {
    /// Funds the grant and reclaims the unvested part on revocation.
    pub grantor: Address,
    /// Receives the vested tokens.
    pub beneficiary: Address,
    /// The token being vested.
    pub token: Address,
    /// Total ever funded.
    pub total: i128,
    /// Ledger timestamp at which the schedule begins.
    pub start: u64,
    /// Seconds after `start` before anything vests.
    pub cliff: u64,
    /// Seconds after `start` at which the grant is fully vested.
    pub duration: u64,
    /// Whether the grantor may reclaim the unvested portion.
    pub revocable: bool,
    /// Total ever claimed by the beneficiary.
    pub claimed: i128,
    /// Total ever returned to the grantor on revocation.
    pub returned: i128,
    /// When the grant was revoked, if it was. Vesting stops here.
    pub revoked_at: Option<u64>,
}

impl Grant {
    /// Total vested as of `at`, whether claimed or not.
    ///
    /// Pure: no storage, no environment, so it is testable in isolation.
    /// Revocation freezes the schedule, so vesting is evaluated at the earlier
    /// of `at` and the revocation timestamp.
    ///
    /// # Overflow ceiling
    ///
    /// This function computes `total * elapsed / duration` using [`math::mul_div`],
    /// which errors on overflow rather than wrapping. For a given duration, the
    /// maximum safe `total` is approximately `i128::MAX / duration_in_seconds`.
    ///
    /// For example, with a 10-year vesting duration (~315,576,000 seconds), the
    /// maximum total before `Overflow` is roughly 29 million tokens. Integrators
    /// should size their token's decimals and supply to stay within this limit:
    /// a token with 18 decimal places has a per-vesting-grant ceiling of about
    /// 29 femto-tokens per second of duration, or ~1 million base units for 10 years.
    pub fn vested_amount(&self, at: u64) -> Result<i128, Error> {
        let effective = match self.revoked_at {
            Some(revoked) => at.min(revoked),
            None => at,
        };

        // Nothing before the cliff.
        let cliff_at = self.start.saturating_add(self.cliff);
        if effective < cliff_at {
            return Ok(0);
        }
        // Everything at the end.
        let end = self.end_at();
        if effective >= end {
            return Ok(self.total);
        }

        // Linear in between. `duration` is non-zero because `end > effective
        // >= cliff_at >= start` implies `end > start`. `create` rejects
        // `duration == 0`, so this holds for every stored grant.
        debug_assert!(
            self.duration > 0,
            "linear vesting branch requires non-zero duration"
        );
        // `effective - self.start` cannot underflow: `effective >= cliff_at
        // >= start` always holds (the branches above return before this
        // point whenever `effective < cliff_at`), so `math::sub` here is
        // defense-in-depth rather than a path that can actually return
        // `Underflow`. The `saturating_add` calls above are what make
        // `cliff_at`/`end` well-defined rather than panicking on an
        // unvalidated `Grant`, but `VestingContract::create` additionally
        // rejects any `start + duration` that would need that saturation, so
        // for a grant created through the contract, `end` is always the
        // true, non-saturated sum.
        let elapsed = math::sub(i128::from(effective), i128::from(self.start))?;
        math::mul_div(self.total, elapsed, i128::from(self.duration))
    }

    /// Vested but not yet claimed, as of `at`.
    pub fn claimable_at(&self, at: u64) -> Result<i128, Error> {
        math::sub(self.vested_amount(at)?, self.claimed)
    }

    /// What the contract still holds for this grant.
    pub fn remaining(&self) -> Result<i128, Error> {
        math::sub(math::sub(self.total, self.claimed)?, self.returned)
    }

    /// Whether the grant has been revoked.
    ///
    /// The lifecycle is binary and one-way, so it is modelled by the
    /// `revoked_at` timestamp (which vesting math needs anyway) rather than a
    /// `State` enum like escrow's. See "Lifecycle modeling" in SPEC.md.
    pub fn is_revoked(&self) -> bool {
        self.revoked_at.is_some()
    }

    /// The absolute timestamp at which the cliff falls.
    pub fn cliff_at(&self) -> u64 {
        self.start.saturating_add(self.cliff)
    }

    /// The absolute timestamp at which the grant is fully vested.
    pub fn end_at(&self) -> u64 {
        self.start.saturating_add(self.duration)
    }
}
