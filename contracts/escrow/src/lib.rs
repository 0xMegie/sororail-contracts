#![no_std]
#![doc = r#"
Escrow: funds held by the contract, released on a condition.

One agreement per deployed instance. The depositor funds it; the funds then
leave only by release to the beneficiary, refund to the depositor, or an
arbiter's split after a dispute.

```text
Created ──fund──▶ Funded ──release──▶ Released
                    │
                    ├────refund────▶ Refunded
                    │
                    └───dispute────▶ Disputed ──resolve──▶ Resolved
```

Illegal transitions return an error; they never panic.

## Entry points

| Entry point | Caller | Effect |
|---|---|---|
| `init` | depositor | Records the agreement. Requires depositor auth. |
| `fund` | depositor | Pulls `amount` of the token into the contract. |
| `cancel` | depositor | Backs out of a `Created` escrow before funding. |
| `release` | depositor or arbiter | Pays the beneficiary. |
| `refund` | depositor (after deadline) or arbiter | Returns funds to the depositor. |
| `dispute` | depositor or beneficiary | Freezes a funded escrow. Needs an arbiter. |
| `resolve` | arbiter | Splits funds per `split_bps`. Only from `Disputed`. |
| `state`, `get`, `bump` | anyone | Reads the lifecycle state or agreement; extends TTL. |

See [`Error`] for the typed failures each entry point can return.
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

pub use contract::{EscrowContract, EscrowContractClient};
pub use errors::Error;
pub use types::{Escrow, State};

#[cfg(test)]
mod test;
