use soroban_sdk::contracttype;
use sororail_common::impl_single_position_storage;

use crate::types::Authorization;

#[contracttype]
pub enum DataKey {
    /// The single authorization held by this instance.
    Auth,
}

impl_single_position_storage!(Authorization, DataKey::Auth);
