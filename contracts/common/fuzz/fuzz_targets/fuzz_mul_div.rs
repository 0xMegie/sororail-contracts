//! Differential fuzz target for `sororail_common::math::mul_div`.
//!
//! Drives `mul_div` with random `i128` inputs and checks it against a
//! `BigInt` reference computed without any intermediate overflow. `mul_div`
//! is documented to report `Error::Overflow` on an overflowing *intermediate*
//! product even when the final `a * b / d` would fit in `i128` — that is
//! intentional (see the doc comment on `mul_div`), so this target only
//! requires that a returned `Ok` value matches the reference exactly, and
//! that no input ever produces anything other than the documented error
//! variants.
#![no_main]

use libfuzzer_sys::fuzz_target;
use num_bigint::BigInt;
use sororail_common::{math, Error};

fuzz_target!(|data: (i128, i128, i128)| {
    let (a, b, d) = data;
    let result = math::mul_div(a, b, d);

    if d == 0 {
        assert_eq!(result, Err(Error::DivisionByZero));
        return;
    }

    match result {
        Ok(value) => {
            let reference = (BigInt::from(a) * BigInt::from(b)) / BigInt::from(d);
            assert_eq!(
                BigInt::from(value),
                reference,
                "mul_div({a}, {b}, {d}) = {value}, but the bigint reference is {reference}",
            );
        }
        Err(Error::Overflow) => {
            // Documented: an overflowing intermediate product is reported
            // even if the true quotient would have fit in i128.
        }
        Err(other) => {
            panic!("mul_div({a}, {b}, {d}) returned unexpected error {other:?}");
        }
    }
});
