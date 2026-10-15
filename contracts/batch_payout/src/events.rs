//! Batch payout events.
//!
//! Follows the org convention in `sororail_common::events`.

use soroban_sdk::{contractevent, Address};

/// A batch completed. Emitted once per batch, not once per recipient.
///
/// Indexers must correlate the token contract's `transfer` events in the same
/// transaction to recover each recipient and amount. This event intentionally
/// does not duplicate those transfers, keeping large payrolls compact.
#[contractevent(topics = ["batch_payout", "executed"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Executed {
    #[topic]
    pub funder: Address,
    pub token: Address,
    pub count: u32,
    pub total: i128,
}
