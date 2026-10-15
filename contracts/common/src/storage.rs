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
