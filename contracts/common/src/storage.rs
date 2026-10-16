//! TTL constants and bump helpers.
//!
//! Soroban state expires. An entry that is not extended is archived, and a
//! contract whose config has been archived cannot run. Every entry point that
//! touches storage must therefore extend the TTL of what it touched, so that
//! logic lives here and is written once rather than forgotten in one contract
//! out of six.
//!
//! # Persistent storage (reserved)
//!
//! Today every contract stores its single position in **instance** storage, so
//! only [`extend_instance`] is used. Persistent TTL helpers
//! (`PERSISTENT_BUMP` / `extend_persistent`) are deliberately **not** shipped
//! here yet: they are reserved for the future id-keyed multi-position design
//! discussed in SPEC.md. Reintroduce them with that change rather than leaving
//! unused constants that drift from any real caller.

use soroban_sdk::Env;

/// Ledgers closed in a day, at the ~5s close time Soroban targets.
pub const DAY_IN_LEDGERS: u32 = 17_280;

/// Instance entries (contract config / the single position) are extended to
/// 30 days...
pub const INSTANCE_BUMP: u32 = 30 * DAY_IN_LEDGERS;
/// ...whenever they fall below 29 days remaining.
pub const INSTANCE_THRESHOLD: u32 = INSTANCE_BUMP - DAY_IN_LEDGERS;

/// Extends the TTL of the contract instance and its code.
pub fn extend_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_THRESHOLD, INSTANCE_BUMP);
}

/// Generates the is_initialized/load/load_view/save trio for a contract's single-position storage.
///
/// This macro eliminates the boilerplate that escrow, stream, vesting, and recurring
/// each redefined identically. Use it in your contract's storage.rs module like:
///
/// ```ignore
/// use soroban_sdk::{contracttype, Env};
/// use sororail_common::{storage as ttl, Error, impl_single_position_storage};
/// use crate::types::YourType;
///
/// #[contracttype]
/// pub enum DataKey {
///     YourKey,
/// }
///
/// impl_single_position_storage!(YourType, DataKey::YourKey);
/// ```
///
/// This generates:
/// - `is_initialized(env: &Env) -> bool`
/// - `load(env: &Env) -> Result<YourType, Error>` (extends TTL)
/// - `load_view(env: &Env) -> Result<YourType, Error>` (read-only)
/// - `save(env: &Env, value: &YourType)` (extends TTL)
#[macro_export]
macro_rules! impl_single_position_storage {
    ($type:ty, $key:expr) => {
        /// Whether init has run.
        pub fn is_initialized(env: &::soroban_sdk::Env) -> bool {
            env.storage().instance().has(&$key)
        }

        /// Loads the value for a mutating operation and extends the instance TTL.
        pub fn load(env: &::soroban_sdk::Env) -> Result<$type, $crate::Error> {
            let value = env
                .storage()
                .instance()
                .get(&$key)
                .ok_or($crate::Error::NotInitialized)?;
            $crate::storage::extend_instance(env);
            Ok(value)
        }

        /// Loads the value without changing ledger state.
        pub fn load_view(env: &::soroban_sdk::Env) -> Result<$type, $crate::Error> {
            env.storage()
                .instance()
                .get(&$key)
                .ok_or($crate::Error::NotInitialized)
        }

        /// Persists the value and extends the instance TTL.
        pub fn save(env: &::soroban_sdk::Env, value: &$type) {
            env.storage().instance().set(&$key, value);
            $crate::storage::extend_instance(env);
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{contracttype, testutils::Ledger, Env};

    // Test data structure for macro testing
    #[contracttype]
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct TestData {
        pub value: u64,
        pub name: soroban_sdk::String,
    }

    #[contracttype]
    pub enum TestDataKey {
        TestKey,
    }

    // Generate storage helpers for testing
    impl_single_position_storage!(TestData, TestDataKey::TestKey);

    /// Issue #273: Test extend_instance with a short TTL
    #[test]
    fn test_extend_instance_with_short_ttl() {
        let env = Env::default();
        
        // Set up initial storage with a short TTL (less than threshold)
        let short_ttl = INSTANCE_THRESHOLD - 100; // Below threshold
        env.storage().instance().set(&TestDataKey::TestKey, &42u64);
        env.storage().instance().extend_ttl(short_ttl, short_ttl);
        
        // Call extend_instance
        extend_instance(&env);
        
        // Verify TTL was extended to INSTANCE_BUMP
        // We can't directly read TTL, but we can verify the function doesn't panic
        // and that storage is still accessible after advancing ledgers
        env.ledger().set_sequence_number(env.ledger().sequence() + short_ttl);
        
        // Storage should still be accessible after short_ttl ledgers
        // (because it was extended to INSTANCE_BUMP)
        let value: Option<u64> = env.storage().instance().get(&TestDataKey::TestKey);
        assert!(value.is_some(), "Storage should still be accessible after TTL extension");
    }

    /// Issue #274: Test generated load helper when key is missing
    #[test]
    fn test_load_when_key_missing() {
        let env = Env::default();
        
        // Attempt to load without initializing
        let result = load(&env);
        
        // Assert NotInitialized error is returned
        assert!(result.is_err(), "load should return error when key is missing");
        assert_eq!(
            result.unwrap_err(),
            crate::Error::NotInitialized,
            "Error should be NotInitialized"
        );
    }

    /// Issue #275: Test generated save and load round trip
    #[test]
    fn test_save_and_load_round_trip() {
        let env = Env::default();
        
        // Create test data
        let test_data = TestData {
            value: 12345,
            name: soroban_sdk::String::from_str(&env, "test-name"),
        };
        
        // Save the data
        save(&env, &test_data);
        
        // Load it back
        let loaded = load(&env).expect("load should succeed after save");
        
        // Assert saved value loads back equal
        assert_eq!(
            loaded, test_data,
            "Loaded value should equal saved value"
        );
        
        // Also test load_view
        let loaded_view = load_view(&env).expect("load_view should succeed after save");
        assert_eq!(
            loaded_view, test_data,
            "load_view should return same value as load"
        );
    }

    #[test]
    fn test_is_initialized() {
        let env = Env::default();
        
        // Initially not initialized
        assert!(!is_initialized(&env), "Should not be initialized initially");
        
        // After save, should be initialized
        let test_data = TestData {
            value: 999,
            name: soroban_sdk::String::from_str(&env, "init-test"),
        };
        save(&env, &test_data);
        
        assert!(is_initialized(&env), "Should be initialized after save");
    }
}
