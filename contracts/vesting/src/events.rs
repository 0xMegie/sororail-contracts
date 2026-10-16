//! Vesting events.
//!
//! Follows the org convention in `sororail_common::events`.

use soroban_sdk::{contractevent, Address};

/// Emitted once by `contract::create`, at grant creation, after the grant is
/// saved and funds are transferred in.
#[contractevent(topics = ["vesting", "created"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Created {
    #[topic]
    pub grantor: Address,
    pub beneficiary: Address,
    pub token: Address,
    pub total: i128,
    pub start: u64,
    pub cliff: u64,
    pub duration: u64,
    pub revocable: bool,
}

/// Emitted by `contract::claim` each time the beneficiary withdraws the
/// currently-vested, unclaimed balance. Can fire multiple times over the
/// life of a grant, once per successful claim.
#[contractevent(topics = ["vesting", "claimed"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Claimed {
    #[topic]
    pub beneficiary: Address,
    pub amount: i128,
    /// Cumulative claimed after this call.
    pub total_claimed: i128,
}

/// Emitted once by `contract::revoke`, when the grantor revokes a revocable
/// grant. Fires at most once per grant: a second `revoke` call is rejected
/// with `Error::VestingRevoked` before this event would be emitted again.
/// The vested portion remains claimable by the beneficiary afterward.
#[contractevent(topics = ["vesting", "revoked"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Revoked {
    #[topic]
    pub grantor: Address,
    pub returned_to_grantor: i128,
    pub still_claimable: i128,
    pub revoked_at: u64,
}
