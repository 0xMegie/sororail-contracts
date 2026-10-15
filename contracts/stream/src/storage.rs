use soroban_sdk::contracttype;
use sororail_common::impl_single_position_storage;

use crate::types::Stream;

#[contracttype]
pub enum DataKey {
    /// The single stream held by this instance.
    Stream,
}

impl_single_position_storage!(Stream, DataKey::Stream);
