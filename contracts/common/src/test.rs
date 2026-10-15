// Test fixtures do plain arithmetic on known-small constants; the checked-math
// rule is for contract code.
#![allow(clippy::arithmetic_side_effects)]

use soroban_sdk::{testutils::Address as _, Address, Env};

use crate::{
    auth,
    math::{self, BPS_DENOMINATOR, MAX_BPS},
    storage, Error,
};

// The protocol's ceiling on how far a TTL may be extended (~6 months).
const MAX_TTL: u32 = 3_110_400;

// ---------------------------------------------------------------- math

#[test]
fn add_sub_mul_are_checked() {
    assert_eq!(math::add(2, 3), Ok(5));
    assert_eq!(math::sub(3, 2), Ok(1));
    assert_eq!(math::mul(3, 4), Ok(12));

    assert_eq!(math::add(i128::MAX, 1), Err(Error::Overflow));
    assert_eq!(math::sub(i128::MIN, 1), Err(Error::Underflow));
    assert_eq!(math::mul(i128::MAX, 2), Err(Error::Overflow));
}

#[test]
fn div_rejects_zero_divisor() {
    assert_eq!(math::div(10, 2), Ok(5));
    assert_eq!(math::div(10, 0), Err(Error::DivisionByZero));
    // i128::MIN / -1 is the one division that overflows.
    assert_eq!(math::div(i128::MIN, -1), Err(Error::Overflow));
}

#[test]
fn mul_div_reports_overflow_rather_than_truncating() {
    assert_eq!(math::mul_div(100, 50, 200), Ok(25));
    // Rounds toward zero, never up.
    assert_eq!(math::mul_div(10, 1, 3), Ok(3));
    assert_eq!(math::mul_div(i128::MAX, 2, 2), Err(Error::Overflow));
}

#[test]
fn mul_bps_rejects_out_of_range() {
    assert_eq!(math::mul_bps(1_000, 0), Ok(0));
    assert_eq!(math::mul_bps(1_000, 2_500), Ok(250));
    assert_eq!(math::mul_bps(1_000, MAX_BPS), Ok(1_000));
    assert_eq!(
        math::mul_bps(1_000, MAX_BPS + 1),
        Err(Error::InvalidBasisPoints)
    );
}

#[test]
fn split_bps_conserves_the_total_exactly() {
    // The invariant escrow dispute resolution rests on: no rounding leakage,
    // at any split, for any amount -- including amounts that divide badly.
    let amounts = [
        0i128,
        1,
        2,
        3,
        7,
        99,
        100,
        101,
        9_999,
        10_001,
        i128::MAX / 10_000,
    ];
    let splits = [0u32, 1, 3, 50, 1_234, 5_000, 6_667, 9_999, 10_000];

    for amount in amounts {
        for bps in splits {
            let (a, b) = math::split_bps(amount, bps).unwrap();
            assert_eq!(
                math::add(a, b).unwrap(),
                amount,
                "split of {amount} at {bps}bps did not conserve"
            );
            assert!(
                a >= 0 && b >= 0,
                "split of {amount} at {bps}bps went negative"
            );
        }
    }
}

#[test]
fn split_bps_endpoints_are_whole() {
    assert_eq!(math::split_bps(777, 0), Ok((0, 777)));
    assert_eq!(math::split_bps(777, MAX_BPS), Ok((777, 0)));
}

/// Non-terminating fractions truncate in `mul_bps`, but the remainder leg of
/// `split_bps` must still reconstruct the original amount exactly.
#[test]
fn split_bps_conserves_on_non_terminating_fractions() {
    // 3 * 3333 / 10000 truncates to 0; 1 * 5000 / 10000 truncates to 0;
    // 7 * 1 / 10000 truncates to 0. The second leg carries the dust.
    assert_eq!(math::split_bps(3, 3_333), Ok((0, 3)));
    assert_eq!(math::split_bps(1, 5_000), Ok((0, 1)));
    assert_eq!(math::split_bps(7, 1), Ok((0, 7)));
    for (amount, bps) in [(3i128, 3_333u32), (1, 5_000), (7, 1)] {
        let (first, second) = math::split_bps(amount, bps).unwrap();
        assert_eq!(
            math::add(first, second).unwrap(),
            amount,
            "split of {amount} at {bps}bps did not conserve"
        );
    }
}

#[test]
fn amount_guards() {
    assert_eq!(math::require_positive(1), Ok(()));
    assert_eq!(math::require_positive(0), Err(Error::InvalidAmount));
    assert_eq!(math::require_positive(-1), Err(Error::InvalidAmount));

    assert_eq!(math::require_non_negative(0), Ok(()));
    assert_eq!(math::require_non_negative(-1), Err(Error::InvalidAmount));
}

#[test]
fn min_picks_the_smaller() {
    assert_eq!(math::min(3, 9), 3);
    assert_eq!(math::min(9, 3), 3);
    assert_eq!(math::min(-1, 0), -1);
}

#[test]
fn bps_denominator_matches_max_bps() {
    assert_eq!(BPS_DENOMINATOR, i128::from(MAX_BPS));
}

// ---------------------------------------------------------------- storage

// These relationships hold between constants, so they are checked at compile
// time rather than at test time: a violation should fail the build, not wait
// for someone to run the suite.
const _: () = {
    assert!(storage::INSTANCE_THRESHOLD < storage::INSTANCE_BUMP);
    // Asking for more than the protocol ceiling is an error at runtime.
    assert!(storage::INSTANCE_BUMP <= MAX_TTL);
};

/// Pins every released `Error` discriminant. Clients decode failures by these
/// integers; renumbering or removing a variant must fail this test rather than
/// relying on review alone. Append new variants at the end of their range and
/// extend this table in the same PR.
#[test]
fn error_discriminants_match_the_published_abi_table() {
    let table: &[(Error, u32)] = &[
        (Error::AlreadyInitialized, 1),
        (Error::NotInitialized, 2),
        (Error::Unauthorized, 3),
        (Error::InvalidAmount, 4),
        (Error::InvalidTimeRange, 5),
        (Error::Overflow, 6),
        (Error::Underflow, 7),
        (Error::DivisionByZero, 8),
        (Error::InvalidBasisPoints, 9),
        (Error::InvalidState, 10),
        (Error::DeadlineNotReached, 11),
        (Error::DeadlinePassed, 12),
        (Error::InsufficientBalance, 13),
        (Error::InvalidDuration, 14),
        (Error::IdenticalParties, 15),
        (Error::EscrowNotFundable, 20),
        (Error::EscrowNotFunded, 21),
        (Error::EscrowClosed, 22),
        (Error::EscrowNoArbiter, 23),
        (Error::EscrowNotDisputed, 24),
        (Error::EscrowAlreadyDisputed, 25),
        (Error::StreamNotFound, 40),
        (Error::StreamCancelled, 41),
        (Error::StreamNotCancellable, 42),
        (Error::StreamInsufficientAccrued, 43),
        (Error::StreamNotExtendable, 44),
        (Error::VestingNotFound, 60),
        (Error::VestingCliffNotReached, 61),
        (Error::VestingNotRevocable, 62),
        (Error::VestingRevoked, 63),
        (Error::VestingCliffAfterEnd, 64),
        (Error::VestingNothingToClaim, 65),
        (Error::RecurringNotFound, 80),
        (Error::RecurringCancelled, 81),
        (Error::RecurringPeriodNotElapsed, 82),
        (Error::RecurringExhausted, 83),
        (Error::BatchEmpty, 100),
        (Error::BatchTooLarge, 101),
    ];
    for &(variant, code) in table {
        assert_eq!(
            variant as u32, code,
            "{variant:?} was renumbered away from ABI code {code}"
        );
    }
}

// ---------------------------------------------------------------- auth
//
// Only the rejection paths are unit-tested here. The accept path calls
// `require_auth`, which needs a real contract invocation context, so it is
// covered in each contract's own tests.

#[test]
fn require_reports_the_given_error() {
    assert_eq!(auth::require(true, Error::Unauthorized), Ok(()));
    assert_eq!(
        auth::require(false, Error::InvalidState),
        Err(Error::InvalidState)
    );
}

#[test]
fn require_auth_as_rejects_a_different_address() {
    let env = Env::default();
    let caller = Address::generate(&env);
    let expected = Address::generate(&env);
    assert_eq!(
        auth::require_auth_as(&caller, &expected),
        Err(Error::Unauthorized)
    );
}

#[test]
fn require_auth_either_rejects_an_outsider() {
    let env = Env::default();
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let outsider = Address::generate(&env);
    assert_eq!(
        auth::require_auth_either(&outsider, &a, &b),
        Err(Error::Unauthorized)
    );
}

#[test]
fn require_auth_either_opt_rejects_when_optional_party_absent() {
    let env = Env::default();
    let a = Address::generate(&env);
    let other = Address::generate(&env);
    assert_eq!(
        auth::require_auth_either_opt(&other, &a, &None),
        Err(Error::Unauthorized)
    );
}

#[test]
fn require_auth_one_of_rejects_an_outsider() {
    let env = Env::default();
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let outsider = Address::generate(&env);
    let allowed = auth::allow_list(&env, &[&a, &b]);
    assert_eq!(
        auth::require_auth_one_of(&outsider, &allowed),
        Err(Error::Unauthorized)
    );
}
