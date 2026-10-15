use soroban_sdk::contracttype;
use sororail_common::impl_single_position_storage;

use crate::types::Escrow;

#[contracttype]
pub enum DataKey {
    /// The single escrow agreement held by this instance.
    Escrow,
}

impl_single_position_storage!(Escrow, DataKey::Escrow);
