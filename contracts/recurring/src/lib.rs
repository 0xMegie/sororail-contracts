#![no_std]
#![doc = r#"
Recurring: pull-based authorization for subscriptions.

The payer authorizes a cap and a cadence; the payee pulls within it. One
authorization per deployed instance.

This contract never holds funds. The payer grants it an allowance on the token
with `approve`, and each `charge` moves money directly from payer to payee via
`transfer_from`. Revoking that allowance stops charges immediately, with or
without cancelling here.

**Skipped periods are forfeited, not banked.** The next charge is scheduled at
`now + period_seconds` each time, so a payee who forgets to charge for three
months cannot then take three payments at once. That is a deliberate
consumer-protection choice.

## Entry points

| Entry point | Caller | Effect |
|---|---|---|
| `authorize` | payer | Records the cap and cadence. Requires payer auth. |
| `charge` | payee | Pulls one period's amount from payer to payee via `transfer_from`. |
| `cancel` | payer or payee | Stops future charges. Revoking the token allowance also stops them. |
| `next_chargeable_at`, `is_chargeable`, `remaining_periods` | anyone | Reads the schedule and what is still allowed. |
| `bump`, `get` | anyone | Extends TTL; reads the full authorization record. |

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

pub use contract::{RecurringContract, RecurringContractClient};
pub use errors::Error;
pub use types::Authorization;

#[cfg(test)]
mod test;
