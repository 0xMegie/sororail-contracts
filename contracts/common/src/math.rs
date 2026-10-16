//! Checked money math.
//!
//! Every operation here returns [`Error`] rather than wrapping, saturating or
//! panicking. Saturation is the dangerous default for balances: it silently
//! converts an overflow into a wrong-but-plausible number, and a payment
//! contract that reports a wrong balance is worse than one that refuses to act.
//!
//! # Basis-point math: single-consumer functions in a shared crate
//!
//! [`mul_bps`] and [`split_bps`] are currently used only by `escrow::resolve`,
//! but they live here rather than in `contracts/escrow/src/` for two reasons:
//!
//! 1. **Correctness by proximity**: The no-rounding-leakage invariant that
//!    `split_bps` provides is subtle and security-critical for dispute
//!    resolution. Keeping it alongside the other checked math makes that
//!    property visible in the shared test suite and documents it as a
//!    general-purpose building block, not an escrow implementation detail.
//!
//! 2. **Future reuse without duplication**: Any future contract that needs
//!    proportional splits (fee distribution, profit sharing, multi-party
//!    settlements) can use these immediately without rediscovering the
//!    remainder-by-subtraction pattern or reintroducing the tests.
//!
//! If no other contract adopts them by the time the codebase ships 1.0, we may
//! revisit this and move them into escrow. Until then, the cost of a few extra
//! lines in a shared crate is lower than the risk of reimplementing bps math
//! incorrectly in a future contract.

use crate::Error;

/// Denominator for basis-point math. 10000 bps == 100%.
pub const BPS_DENOMINATOR: i128 = 10_000;

/// Returns the uses left under an optional cap, saturating at zero.
///
/// `None` represents an unbounded count. Shared by types that expose remaining
/// uses so capped behavior stays consistent across contracts.
pub const fn remaining_count(cap: Option<u32>, used: u32) -> Option<u32> {
    match cap {
        Some(cap) => Some(cap.saturating_sub(used)),
        None => None,
    }
}

/// Maximum valid basis points.
pub const MAX_BPS: u32 = 10_000;

#[inline]
pub fn add(a: i128, b: i128) -> Result<i128, Error> {
    a.checked_add(b).ok_or(Error::Overflow)
}

#[inline]
pub fn sub(a: i128, b: i128) -> Result<i128, Error> {
    a.checked_sub(b).ok_or(Error::Underflow)
}

#[inline]
pub fn mul(a: i128, b: i128) -> Result<i128, Error> {
    a.checked_mul(b).ok_or(Error::Overflow)
}

#[inline]
pub fn div(a: i128, b: i128) -> Result<i128, Error> {
    if b == 0 {
        return Err(Error::DivisionByZero);
    }
    a.checked_div(b).ok_or(Error::Overflow)
}

/// `a * b / d`, rounded toward zero.
///
/// The intermediate product is computed in `i128` and errors on overflow
/// rather than widening. Callers doing proportional math over very large
/// totals (`total * elapsed / duration`) should be aware that an overflow is
/// reported as [`Error::Overflow`], not silently truncated.
///
/// # Examples
///
/// ```
/// use sororail_common::math;
///
/// assert_eq!(math::mul_div(1_000, 25, 100), Ok(250));
/// ```
pub fn mul_div(a: i128, b: i128, d: i128) -> Result<i128, Error> {
    div(mul(a, b)?, d)
}

/// `amount * bps / 10000`, rounded toward zero.
///
/// Currently used only by `escrow::resolve`. See module docs for rationale.
///
/// # Examples
///
/// ```
/// use sororail_common::math;
///
/// assert_eq!(math::mul_bps(1_000_000, 250), Ok(25_000));
/// ```
pub fn mul_bps(amount: i128, bps: u32) -> Result<i128, Error> {
    if bps > MAX_BPS {
        return Err(Error::InvalidBasisPoints);
    }
    mul_div(amount, i128::from(bps), BPS_DENOMINATOR)
}

/// Splits `amount` into `(first, second)` where `first` is `bps` of the total.
///
/// `first` is `mul_bps(amount, bps)`, truncated toward zero (see
/// [`mul_bps`]). `second` is computed as `amount - first` rather than as
/// `mul_bps(amount, 10000 - bps)`, so any remainder left over by that
/// truncation always lands on `second` and the two parts always sum to
/// exactly `amount` with no rounding leakage. Callers can rely on `second`
/// being `>= amount * (10000 - bps) / 10000`, never less. This is the
/// property escrow dispute resolution depends on.
///
/// Currently used only by `escrow::resolve`. See module docs for rationale.
///
/// # Examples
///
/// ```
/// use sororail_common::math;
///
/// let (fee, remainder) = math::split_bps(101, 3_333)?;
/// assert_eq!(fee + remainder, 101);
/// # Ok::<(), sororail_common::Error>(())
/// ```
pub fn split_bps(amount: i128, bps: u32) -> Result<(i128, i128), Error> {
    let first = mul_bps(amount, bps)?;
    let second = sub(amount, first)?;
    Ok((first, second))
}

/// Errors unless `amount` is strictly positive.
#[inline]
pub fn require_positive(amount: i128) -> Result<(), Error> {
    if amount <= 0 {
        return Err(Error::InvalidAmount);
    }
    Ok(())
}

/// Errors unless `amount` is zero or positive.
#[inline]
pub fn require_non_negative(amount: i128) -> Result<(), Error> {
    if amount < 0 {
        return Err(Error::InvalidAmount);
    }
    Ok(())
}

/// Returns the smaller of two values.
#[inline]
pub fn min(a: i128, b: i128) -> i128 {
    if a < b {
        a
    } else {
        b
    }
}

#[cfg(test)]
mod remaining_count_tests {
    use super::remaining_count;

    #[test]
    fn cap_remaining_count_saturates_and_supports_no_cap() {
        assert_eq!(remaining_count(Some(3), 1), Some(2));
        assert_eq!(remaining_count(Some(3), 3), Some(0));
        assert_eq!(remaining_count(Some(3), 4), Some(0));
        assert_eq!(remaining_count(None, u32::MAX), None);
    }
}

#[cfg(test)]
mod tests {
    // This seeded property-test generator intentionally uses wrapping,
    // truncating PRNG arithmetic; contract arithmetic remains checked above.
    #![allow(clippy::arithmetic_side_effects)]

    use super::*;

    /// A small xorshift PRNG. `rand`/`proptest` aren't workspace dependencies
    /// (see `CONTRIBUTING.md`'s supply-chain policy), and this is plenty for
    /// a deterministic, seeded sweep over `split_bps`'s input space.
    struct Xorshift(u64);

    impl Xorshift {
        fn next_u64(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }

        /// Uniform in `[lo, hi]`.
        fn next_i128_in(&mut self, lo: i128, hi: i128) -> i128 {
            let span = hi
                .checked_sub(lo)
                .and_then(|difference| difference.checked_add(1))
                .and_then(|difference| u128::try_from(difference).ok())
                .expect("range should be positive and fit in u128");
            let offset = i128::try_from(u128::from(self.next_u64()) % span)
                .expect("offset should fit in i128");
            lo.checked_add(offset)
                .expect("generated value should remain within the requested range")
        }

        fn next_bps(&mut self) -> u32 {
            u32::try_from(self.next_u64() % u64::from(MAX_BPS).saturating_add(1))
                .expect("generated value should not exceed MAX_BPS")
        }
    }

    /// `split_bps`'s whole contract is that the two shares it returns always
    /// sum back to exactly the input, with no rounding leakage -- escrow
    /// dispute resolution depends on this holding for every valid `amount`
    /// and `bps`, not just the hand-picked doctest example.
    #[test]
    fn split_bps_sums_to_input_across_random_inputs() {
        let mut rng = Xorshift(0x5eed_u64.wrapping_add(1));

        for _ in 0..10_000 {
            let amount = rng.next_i128_in(0, 1_000_000_000_000);
            let bps = rng.next_bps();

            let (first, second) = split_bps(amount, bps)
                .unwrap_or_else(|err| panic!("split_bps({amount}, {bps}) failed: {err:?}"));

            assert_eq!(
                first + second,
                amount,
                "split_bps({amount}, {bps}) = ({first}, {second}), which does not sum to {amount}"
            );
            assert!(first >= 0, "first share went negative: {first}");
            assert!(second >= 0, "second share went negative: {second}");
        }
    }

    #[test]
    fn split_bps_rejects_bps_over_max() {
        assert_eq!(split_bps(100, MAX_BPS + 1), Err(Error::InvalidBasisPoints));
    }
}
