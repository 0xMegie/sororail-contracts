#![no_std]
#![doc = r#"
Vesting: scheduled release against a schedule, with a cliff.

One grant per deployed instance, fully funded up front. Nothing vests before
the cliff; after it, vesting is linear until fully vested at `start + duration`.
A revocable grant lets the grantor reclaim the unvested remainder, while the
vested portion stays claimable by the beneficiary.

`cliff` and `duration` are spans in seconds from `start`, not absolute
timestamps.

The correctness property to hold onto:

```text
claimed + returned + remaining == total
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

pub use contract::{VestingContract, VestingContractClient};
pub use errors::Error;
pub use types::Grant;

#[cfg(test)]
mod test;
