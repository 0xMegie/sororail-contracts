use soroban_sdk::contracttype;
use sororail_common::impl_single_position_storage;

use crate::types::Escrow;

#[contracttype]
pub enum DataKey {
    /// The single escrow agreement held by this instance. First written by
    /// `init`, then updated in place by every state-changing entry point
    /// that touches the agreement (`cancel`, `fund`, `release`, `refund`,
    /// `dispute`, `resolve`).
    Escrow,
}

impl_single_position_storage!(Escrow, DataKey::Escrow);
