#![no_std]
#![doc = r#"
Batch payout: one transaction, many recipients. The payroll primitive.

**This contract is stateless.** It holds no storage and no funds -- there is no
`init`, and therefore no `storage.rs` in this crate, unlike its siblings. Each
call pulls straight from the funder to each recipient and leaves nothing
behind, so there is no position to archive and no TTL to extend.

**All-or-nothing.** A single failed transfer aborts the whole invocation and
the host reverts every transfer that preceded it. Nobody is paid unless
everybody is. The contract relies on that host guarantee rather than
attempting to unwind by hand, and deliberately never catches a transfer error.
"#]

#[cfg(test)]
extern crate std;

soroban_sdk::contractmeta!(key = "version", val = env!("CARGO_PKG_VERSION"));
soroban_sdk::contractmeta!(key = "git_commit", val = env!("SORORAIL_GIT_COMMIT"));

pub mod contract;
pub mod errors;
pub mod events;
pub mod types;

pub use contract::{BatchPayoutContract, BatchPayoutContractClient};
pub use errors::Error;
pub use types::{Payment, Receipt, MAX_RECIPIENTS};

#[cfg(test)]
mod test;
