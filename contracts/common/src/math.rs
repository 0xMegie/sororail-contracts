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
/// `second` is computed as the remainder rather than as `10000 - bps`, so the
/// two parts always sum to exactly `amount` with no rounding leakage. This is
/// the property escrow dispute resolution depends on.
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
