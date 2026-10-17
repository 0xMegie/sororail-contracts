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

## Entry points

| Entry point | Caller | Effect |
|---|---|---|
| `create` | sender | Records the stream and pulls the full `deposited` amount in. |
| `withdraw` | recipient | Pays out accrued, unwithdrawn funds (all of them if `amount` is `None`). |
| `cancel` | sender | Ends a cancellable stream; returns unaccrued funds to the sender. |
| `top_up` | sender | Adds funds to the deposit. The amount must keep the per-second rate exact. |
| `extend` | sender | Moves `stop` later and pulls the extra funds the longer span needs. |
| `balance_of`, `remaining` | anyone | Reads accrued-but-unwithdrawn or unstreamed balances. |
| `bump`, `get` | anyone | Extends TTL; reads the full stream record. |

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

pub use contract::{StreamContract, StreamContractClient};
pub use errors::Error;
pub use types::Stream;

#[cfg(test)]
mod test;
