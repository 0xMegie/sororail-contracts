//! Authorization guards.
//!
//! Missing authorization checks are the most common Soroban vulnerability
//! class, so the intent is that a privileged entry point reads as one obvious
//! line rather than an ad-hoc comparison chain.
//!
//! Note the ordering rule these helpers enforce: **membership is checked
//! before `require_auth`**. Checking auth first would let an unrelated address
//! that happens to hold a valid signature reach the authorization prompt.

use soroban_sdk::{Address, Env, Vec};

use crate::Error;

/// Errors with `err` unless `cond` holds.
#[inline]
pub fn require(cond: bool, err: Error) -> Result<(), Error> {
    if cond {
        Ok(())
    } else {
        Err(err)
    }
}

/// Requires that `caller` is `expected`, then requires its authorization.
pub fn require_auth_as(caller: &Address, expected: &Address) -> Result<(), Error> {
    require(caller == expected, Error::Unauthorized)?;
    caller.require_auth();
    Ok(())
}

/// Requires that `caller` is one of `allowed`, then requires its authorization.
///
/// `allowed` is expected to be short -- two or three addresses -- so the linear
/// scan is deliberate.
pub fn require_auth_one_of(caller: &Address, allowed: &Vec<Address>) -> Result<(), Error> {
    require(allowed.contains(caller), Error::Unauthorized)?;
    caller.require_auth();
    Ok(())
}

/// Requires that `caller` is either `a` or `b`, then requires its authorization.
pub fn require_auth_either(caller: &Address, a: &Address, b: &Address) -> Result<(), Error> {
    require(caller == a || caller == b, Error::Unauthorized)?;
    caller.require_auth();
    Ok(())
}

/// Requires that `caller` is `a`, or `b` when `b` is present.
///
/// Models the recurring "depositor or the optional arbiter" shape.
pub fn require_auth_either_opt(
    caller: &Address,
    a: &Address,
    b: &Option<Address>,
) -> Result<(), Error> {
    let permitted = caller == a || b.as_ref().is_some_and(|x| caller == x);
    require(permitted, Error::Unauthorized)?;
    caller.require_auth();
    Ok(())
}

/// Convenience for building a short allow-list.
pub fn allow_list(env: &Env, addrs: &[&Address]) -> Vec<Address> {
    let mut v = Vec::new(env);
    for a in addrs {
        v.push_back((*a).clone());
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{contract, testutils::Address as _};

    #[contract]
    struct AuthTestHost;

    /// sdk 27 evaluates `require_auth` against a contract frame, so the
    /// assertions run inside `env.as_contract` on a throwaway host contract.
    fn auth_env() -> (Env, soroban_sdk::Address) {
        let env = Env::default();
        env.mock_all_auths();
        let id = env.register(AuthTestHost, ());
        (env, id)
    }

    /// Issue #272: Test require_auth_either_opt when optional party is present
    #[test]
    fn test_require_auth_either_opt_with_optional_present() {
        let (env, id) = auth_env();

        let party_a = Address::generate(&env);
        let party_b = Address::generate(&env);
        let optional_party = Some(party_b.clone());

        env.as_contract(&id, || {
            // Test with first party (a)
            let result_a = require_auth_either_opt(&party_a, &party_a, &optional_party);
            assert!(result_a.is_ok(), "Should succeed when caller is party_a");

            // Test with optional party (b)
            let result_b = require_auth_either_opt(&party_b, &party_a, &optional_party);
            assert!(
                result_b.is_ok(),
                "Should succeed when caller is optional party_b"
            );
        });
    }

    #[test]
    fn test_require_auth_either_opt_without_optional() {
        let (env, id) = auth_env();

        let party_a = Address::generate(&env);
        let unauthorized = Address::generate(&env);

        env.as_contract(&id, || {
            // Test with only party_a allowed
            let result = require_auth_either_opt(&party_a, &party_a, &None);
            assert!(
                result.is_ok(),
                "Should succeed when caller is party_a and no optional"
            );

            // Test with unauthorized party when optional is None
            let result_fail = require_auth_either_opt(&unauthorized, &party_a, &None);
            assert!(
                result_fail.is_err(),
                "Should fail when caller is neither party_a nor optional"
            );
            assert_eq!(result_fail.unwrap_err(), Error::Unauthorized);
        });
    }

    #[test]
    fn test_require() {
        assert!(require(true, Error::Unauthorized).is_ok());
        assert!(require(false, Error::Unauthorized).is_err());
    }

    #[test]
    fn test_require_auth_as() {
        let (env, id) = auth_env();

        let addr = Address::generate(&env);
        let other = Address::generate(&env);

        env.as_contract(&id, || {
            assert!(require_auth_as(&addr, &addr).is_ok());
            assert!(require_auth_as(&other, &addr).is_err());
        });
    }

    #[test]
    fn test_require_auth_either() {
        let (env, id) = auth_env();

        let addr_a = Address::generate(&env);
        let addr_b = Address::generate(&env);
        let other = Address::generate(&env);

        env.as_contract(&id, || {
            assert!(require_auth_either(&addr_a, &addr_a, &addr_b).is_ok());
            assert!(require_auth_either(&addr_b, &addr_a, &addr_b).is_ok());
            assert!(require_auth_either(&other, &addr_a, &addr_b).is_err());
        });
    }

    #[test]
    fn test_require_auth_one_of() {
        let (env, id) = auth_env();

        let addr_a = Address::generate(&env);
        let addr_b = Address::generate(&env);
        let other = Address::generate(&env);

        let allowed = allow_list(&env, &[&addr_a, &addr_b]);

        env.as_contract(&id, || {
            assert!(require_auth_one_of(&addr_a, &allowed).is_ok());
            assert!(require_auth_one_of(&addr_b, &allowed).is_ok());
            assert!(require_auth_one_of(&other, &allowed).is_err());
        });
    }

    #[test]
    fn test_allow_list() {
        let env = Env::default();

        let addr_a = Address::generate(&env);
        let addr_b = Address::generate(&env);

        let list = allow_list(&env, &[&addr_a, &addr_b]);

        assert_eq!(list.len(), 2);
        assert!(list.contains(&addr_a));
        assert!(list.contains(&addr_b));
    }
}
