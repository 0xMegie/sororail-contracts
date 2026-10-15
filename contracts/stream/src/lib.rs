#![no_std]
#![doc = r#"
Stream: continuous per-second transfer from sender to recipient.

One stream per deployed instance, fully funded up front for its whole declared
span. The recipient withdraws what has accrued; the sender may cancel, if the
stream was created cancellable, taking back only what has not yet accrued.

There is no per-second state. Accrual is a pure function of the stream record
and a ledger timestamp, evaluated at read time.

The correctness property to hold onto:

```text
withdrawn + refunded + remaining == deposited
```

exactly, at every point in the lifecycle, with no rounding leakage.
"#]

#[cfg(test)]
extern crate std;

soroban_sdk::contractmeta!(key = "version", val = env!("CARGO_PKG_VERSION"));
soroban_sdk::contractmeta!(key = "git_commit", val = env!("SORORAIL_GIT_COMMIT"));

pub mod contract;
pub mod errors;
pub mod events;
pub mod storage;
pub mod types;

pub use contract::{StreamContract, StreamContractClient};
pub use errors::Error;
pub use types::Stream;

#[cfg(test)]
mod test;
