//! Stream events.
//!
//! Follows the org convention in `sororail_common::events`: fixed topics
//! `[contract, action]`, the attributed address as the single `#[topic]`
//! field, amounts as data.

use soroban_sdk::{contractevent, Address};

/// A stream was created and funded.
#[contractevent(topics = ["stream", "created"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Created {
    #[topic]
    pub sender: Address,
    pub recipient: Address,
    pub token: Address,
    pub rate_per_second: i128,
    pub start: u64,
    pub stop: u64,
    pub cancellable: bool,
    pub deposited: i128,
}

/// The recipient withdrew accrued funds.
#[contractevent(topics = ["stream", "withdrawn"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Withdrawn {
    #[topic]
    pub recipient: Address,
    pub amount: i128,
    /// Cumulative withdrawn after this call.
    pub total_withdrawn: i128,
}

/// The stream was cancelled: accrued settled to the recipient, remainder
/// returned to the sender.
#[contractevent(topics = ["stream", "cancelled"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Cancelled {
    #[topic]
    pub sender: Address,
    pub settled_to_recipient: i128,
    pub refunded_to_sender: i128,
    pub cancelled_at: u64,
}

/// The sender added funds, extending `stop`.
#[contractevent(topics = ["stream", "topped_up"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToppedUp {
    #[topic]
    pub sender: Address,
    pub amount: i128,
    pub new_stop: u64,
    pub deposited: i128,
}

/// The sender extended `stop`, adding the funds the new span requires.
#[contractevent(topics = ["stream", "extended"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Extended {
    #[topic]
    pub sender: Address,
    pub added: i128,
    /// The `stop` the stream had before this call.
    pub old_stop: u64,
    pub new_stop: u64,
    pub deposited: i128,
}
