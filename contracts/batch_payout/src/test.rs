// Test fixtures do plain arithmetic on known-small constants; the checked-math
// rule is for contract code.
#![allow(clippy::arithmetic_side_effects)]

use soroban_sdk::{
    testutils::{Address as _, Events as _},
    token::{StellarAssetClient, TokenClient},
    vec, Address, Env, Event, Vec,
};
use sororail_common::{testutils::TestEnv, Error};

use crate::{
    contract::{BatchPayoutContract, BatchPayoutContractClient},
    events::Executed,
    types::{Payment, Receipt, MAX_RECIPIENTS},
};

const MINT: i128 = 1_000_000_000;

struct Fixture<'a> {
    env: Env,
    client: BatchPayoutContractClient<'a>,
    token: TokenClient<'a>,
    funder: Address,
}

impl Fixture<'_> {
    fn new() -> Self {
        let te = TestEnv::new();
        let (token, funder) = te.make_token(MINT);
        let token_address = token.address.clone();
        let env = te.env().clone();

        let client = BatchPayoutContractClient::new(&env, &env.register(BatchPayoutContract, ()));
        Fixture {
            token: TokenClient::new(&env, &token_address),
            env,
            client,
            funder,
        }
    }

    fn addresses(&self, n: u32) -> Vec<Address> {
        let mut v = Vec::new(&self.env);
        for _ in 0..n {
            v.push_back(Address::generate(&self.env));
        }
        v
    }

    fn payments(&self, n: u32, amount: i128) -> Vec<Payment> {
        let mut v = Vec::new(&self.env);
        for _ in 0..n {
            v.push_back(Payment {
                to: Address::generate(&self.env),
                amount,
            });
        }
        v
    }
}

// ----------------------------------------------------------------- execute

#[test]
fn execute_pays_each_recipient_their_own_amount() {
    let f = Fixture::new();
    let a = Address::generate(&f.env);
    let b = Address::generate(&f.env);
    let c = Address::generate(&f.env);
    let batch = vec![
        &f.env,
        Payment {
            to: a.clone(),
            amount: 100,
        },
        Payment {
            to: b.clone(),
            amount: 250,
        },
        Payment {
            to: c.clone(),
            amount: 7,
        },
    ];

    let receipt = f.client.execute(&f.funder, &f.token.address, &batch);
    // Capture immediately: subsequent token balance invocations replace the
    // events returned by `events().all()`.
    let all = f.env.events().all();
    let events = all.events();
    assert_eq!(
        receipt,
        Receipt {
            count: 3,
            total: 357
        }
    );
    assert_eq!(f.token.balance(&a), 100);
    assert_eq!(f.token.balance(&b), 250);
    assert_eq!(f.token.balance(&c), 7);
    assert_eq!(f.token.balance(&f.funder), MINT - 357);
    // The contract is a conduit; it holds nothing.
    assert_eq!(f.token.balance(&f.client.address), 0);

    // `execute` emits one token transfer per recipient before its own
    // `Executed`, so take the last event rather than assuming an index.
    let executed_event = events.last().expect("no events emitted");
    let expected = Executed {
        funder: f.funder.clone(),
        token: f.token.address.clone(),
        count: 3,
        total: 357,
    };
    assert_eq!(executed_event, &expected.to_xdr(&f.env, &f.client.address));
}

#[test]
fn execute_rejects_an_empty_batch() {
    let f = Fixture::new();
    let empty: Vec<Payment> = Vec::new(&f.env);
    assert_eq!(
        f.client.try_execute(&f.funder, &f.token.address, &empty),
        Err(Ok(Error::BatchEmpty))
    );
}

#[test]
fn execute_rejects_a_batch_over_the_cap() {
    let f = Fixture::new();
    let batch = f.payments(MAX_RECIPIENTS + 1, 1);
    assert_eq!(
        f.client.try_execute(&f.funder, &f.token.address, &batch),
        Err(Ok(Error::BatchTooLarge))
    );
}

#[test]
fn execute_rejects_non_positive_amounts_before_paying_anyone() {
    let f = Fixture::new();
    let good = Address::generate(&f.env);
    let bad = Address::generate(&f.env);
    for bad_amount in [0i128, -1, i128::MIN] {
        let batch = vec![
            &f.env,
            Payment {
                to: good.clone(),
                amount: 100,
            },
            Payment {
                to: bad.clone(),
                amount: bad_amount,
            },
        ];
        assert_eq!(
            f.client.try_execute(&f.funder, &f.token.address, &batch),
            Err(Ok(Error::InvalidAmount)),
            "amount {bad_amount} was accepted"
        );
        // Validation happens before any transfer, so the valid line in the
        // same batch was never paid either.
        assert_eq!(f.token.balance(&good), 0);
        assert_eq!(f.token.balance(&f.funder), MINT);
    }
}

#[test]
fn execute_rejects_a_zero_amount_payment_inside_a_larger_batch() {
    // The other non-positive-amount test only uses a 2-line batch; this
    // checks the same all-or-nothing validation holds with several valid
    // lines around the bad one, not just a single good line next to it (#264).
    let f = Fixture::new();
    let recipients: std::vec::Vec<Address> = (0..5).map(|_| Address::generate(&f.env)).collect();
    let mut batch = Vec::new(&f.env);
    for (i, to) in recipients.iter().enumerate() {
        batch.push_back(Payment {
            to: to.clone(),
            // The third of five payments is the zero amount.
            amount: if i == 2 { 0 } else { 100 },
        });
    }

    assert_eq!(
        f.client.try_execute(&f.funder, &f.token.address, &batch),
        Err(Ok(Error::InvalidAmount))
    );
    // All-or-nothing: none of the four otherwise-valid lines were paid either.
    for to in &recipients {
        assert_eq!(f.token.balance(to), 0);
    }
    assert_eq!(f.token.balance(&f.funder), MINT);
}

#[test]
fn preview_rejects_a_zero_amount_payment_inside_a_larger_batch() {
    // `preview` runs the same validation as `execute` (#264); nothing to
    // assert about balances since preview never moves funds.
    let f = Fixture::new();
    let mut batch = Vec::new(&f.env);
    for i in 0..5 {
        batch.push_back(Payment {
            to: Address::generate(&f.env),
            amount: if i == 2 { 0 } else { 100 },
        });
    }

    assert_eq!(f.client.try_preview(&batch), Err(Ok(Error::InvalidAmount)));
}

#[test]
fn execute_rejects_a_total_that_overflows() {
    let f = Fixture::new();
    let batch = vec![
        &f.env,
        Payment {
            to: Address::generate(&f.env),
            amount: i128::MAX,
        },
        Payment {
            to: Address::generate(&f.env),
            amount: 1,
        },
    ];
    assert_eq!(
        f.client.try_execute(&f.funder, &f.token.address, &batch),
        Err(Ok(Error::Overflow))
    );
}

#[test]
fn a_batch_the_funder_cannot_afford_pays_nobody() {
    // All-or-nothing: the host reverts the transfers already made when a later
    // one fails on insufficient balance.
    let f = Fixture::new();
    let a = Address::generate(&f.env);
    let b = Address::generate(&f.env);
    let batch = vec![
        &f.env,
        Payment {
            to: a.clone(),
            amount: 100,
        },
        Payment {
            to: b.clone(),
            amount: MINT,
        },
    ];

    assert!(f
        .client
        .try_execute(&f.funder, &f.token.address, &batch)
        .is_err());
    assert_eq!(f.token.balance(&a), 0, "a partial payout survived");
    assert_eq!(f.token.balance(&b), 0);
    assert_eq!(f.token.balance(&f.funder), MINT);
}

/// Same host-revert guarantee as the other contracts: a failed mid-batch
/// transfer leaves no partial mutation (batch_payout is otherwise stateless).
#[test]
fn execute_reverts_cleanly_when_a_transfer_fails() {
    a_batch_the_funder_cannot_afford_pays_nobody();
}

#[test]
fn paying_the_same_address_twice_is_allowed() {
    // Duplicates are legitimate -- two invoices for one contractor -- and are
    // deliberately not rejected on-chain.
    let f = Fixture::new();
    let a = Address::generate(&f.env);
    let batch = vec![
        &f.env,
        Payment {
            to: a.clone(),
            amount: 100,
        },
        Payment {
            to: a.clone(),
            amount: 50,
        },
    ];
    let receipt = f.client.execute(&f.funder, &f.token.address, &batch);
    assert_eq!(receipt.total, 150);
    assert_eq!(f.token.balance(&a), 150);
}

#[test]
#[should_panic]
fn execute_requires_the_funders_authorization() {
    let env = Env::default();
    let funder = Address::generate(&env);
    let issuer = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(issuer).address();
    let client = BatchPayoutContractClient::new(&env, &env.register(BatchPayoutContract, ()));
    let batch = vec![
        &env,
        Payment {
            to: Address::generate(&env),
            amount: 100,
        },
    ];
    client.execute(&funder, &token, &batch);
}

#[test]
fn execute_rejects_payment_to_contract_address() {
    let f = Fixture::new();
    let contract = f.client.address.clone();
    let batch = vec![
        &f.env,
        Payment {
            to: contract,
            amount: 100,
        },
    ];
    assert_eq!(
        f.client.try_execute(&f.funder, &f.token.address, &batch),
        Err(Ok(Error::IdenticalParties))
    );
}

#[test]
fn preview_rejects_payment_to_contract_address() {
    let f = Fixture::new();
    let contract = f.client.address.clone();
    let batch = vec![
        &f.env,
        Payment {
            to: contract,
            amount: 100,
        },
    ];
    assert_eq!(
        f.client.try_preview(&batch),
        Err(Ok(Error::IdenticalParties))
    );
}

#[test]
fn execute_rejects_multiple_invalid_args_in_order() {
    let f = Fixture::new();
    let contract = f.client.address.clone();
    // Both empty batch AND self-payment: size check comes first
    let empty_batch: Vec<Payment> = Vec::new(&f.env);
    assert_eq!(
        f.client
            .try_execute(&f.funder, &f.token.address, &empty_batch),
        Err(Ok(Error::BatchEmpty))
    );

    // Self-payment AND zero amount: self-payment check comes first
    let batch = vec![
        &f.env,
        Payment {
            to: contract,
            amount: 0,
        },
    ];
    assert_eq!(
        f.client.try_execute(&f.funder, &f.token.address, &batch),
        Err(Ok(Error::IdenticalParties))
    );
}

// ----------------------------------------------------------- execute_equal

#[test]
fn execute_equal_pays_everyone_the_same() {
    let f = Fixture::new();
    let recipients = f.addresses(5);
    let receipt = f
        .client
        .execute_equal(&f.funder, &f.token.address, &recipients, &200);

    assert_eq!(
        receipt,
        Receipt {
            count: 5,
            total: 1_000
        }
    );
    for r in recipients.iter() {
        assert_eq!(f.token.balance(&r), 200);
    }
    assert_eq!(f.token.balance(&f.funder), MINT - 1_000);
}

#[test]
fn execute_equal_rejects_a_non_positive_amount() {
    let f = Fixture::new();
    let recipients = f.addresses(3);
    for bad in [0i128, -1, i128::MIN] {
        assert_eq!(
            f.client
                .try_execute_equal(&f.funder, &f.token.address, &recipients, &bad),
            Err(Ok(Error::InvalidAmount)),
            "amount {bad} was accepted"
        );
    }
}

#[test]
fn execute_equal_rejects_an_empty_batch() {
    let f = Fixture::new();
    let empty: Vec<Address> = Vec::new(&f.env);
    assert_eq!(
        f.client
            .try_execute_equal(&f.funder, &f.token.address, &empty, &100),
        Err(Ok(Error::BatchEmpty))
    );
}

#[test]
fn execute_equal_rejects_a_batch_over_the_cap() {
    let f = Fixture::new();
    let recipients = f.addresses(MAX_RECIPIENTS + 1);
    assert_eq!(
        f.client
            .try_execute_equal(&f.funder, &f.token.address, &recipients, &1),
        Err(Ok(Error::BatchTooLarge))
    );
}

#[test]
fn execute_equal_rejects_contract_address_in_recipients() {
    let f = Fixture::new();
    let contract = f.client.address.clone();
    let mut recipients = f.addresses(2);
    recipients.push_back(contract);
    assert_eq!(
        f.client
            .try_execute_equal(&f.funder, &f.token.address, &recipients, &100),
        Err(Ok(Error::IdenticalParties))
    );
}

#[test]
fn execute_equal_rejects_multiple_invalid_args_in_order() {
    let f = Fixture::new();
    let contract = f.client.address.clone();
    // Non-positive amount comes before size check
    assert_eq!(
        f.client
            .try_execute_equal(&f.funder, &f.token.address, &f.addresses(0), &0),
        Err(Ok(Error::InvalidAmount))
    );

    // Size check comes before self-payment check
    let mut oversized = f.addresses(MAX_RECIPIENTS + 1);
    oversized.push_back(contract.clone());
    assert_eq!(
        f.client
            .try_execute_equal(&f.funder, &f.token.address, &oversized, &100),
        Err(Ok(Error::BatchTooLarge))
    );

    // Self-payment check comes before total calculation
    let recipients = vec![&f.env, contract];
    assert_eq!(
        f.client
            .try_execute_equal(&f.funder, &f.token.address, &recipients, &i128::MAX),
        Err(Ok(Error::IdenticalParties))
    );
}

#[test]
fn execute_equal_rejects_a_total_that_overflows() {
    let f = Fixture::new();
    let recipients = f.addresses(2);
    assert_eq!(
        f.client
            .try_execute_equal(&f.funder, &f.token.address, &recipients, &i128::MAX),
        Err(Ok(Error::Overflow))
    );
}

// ----------------------------------------------------------------- preview

#[test]
fn preview_totals_without_moving_anything() {
    let f = Fixture::new();
    let batch = f.payments(4, 25);
    let receipt = f.client.preview(&batch);
    assert_eq!(
        receipt,
        Receipt {
            count: 4,
            total: 100
        }
    );
    assert_eq!(f.token.balance(&f.funder), MINT);
}

#[test]
fn preview_returns_the_correct_receipt_and_moves_nothing() {
    // The existing uniform-amount preview test can't catch a `Receipt` whose
    // `total` happens to equal `count * amount` by coincidence; this uses
    // distinct amounts so `count` and `total` are independently verified,
    // and checks every balance plus the event log, not just the funder's
    // (#265).
    let f = Fixture::new();
    let a = Address::generate(&f.env);
    let b = Address::generate(&f.env);
    let c = Address::generate(&f.env);
    let batch = vec![
        &f.env,
        Payment {
            to: a.clone(),
            amount: 11,
        },
        Payment {
            to: b.clone(),
            amount: 22,
        },
        Payment {
            to: c.clone(),
            amount: 33,
        },
    ];

    let receipt = f.client.preview(&batch);
    assert_eq!(
        receipt,
        Receipt {
            count: 3,
            total: 66
        }
    );

    assert_eq!(f.token.balance(&a), 0);
    assert_eq!(f.token.balance(&b), 0);
    assert_eq!(f.token.balance(&c), 0);
    assert_eq!(f.token.balance(&f.funder), MINT);
    assert_eq!(f.token.balance(&f.client.address), 0);
    // Unlike `execute`, `preview` never publishes an `Executed` event.
    assert!(f.env.events().all().events().is_empty());
}

#[test]
fn preview_rejects_exactly_what_execute_rejects() {
    let f = Fixture::new();
    let empty: Vec<Payment> = Vec::new(&f.env);
    assert_eq!(f.client.try_preview(&empty), Err(Ok(Error::BatchEmpty)));

    let too_many = f.payments(MAX_RECIPIENTS + 1, 1);
    assert_eq!(
        f.client.try_preview(&too_many),
        Err(Ok(Error::BatchTooLarge))
    );

    let bad = vec![
        &f.env,
        Payment {
            to: Address::generate(&f.env),
            amount: 0,
        },
    ];
    assert_eq!(f.client.try_preview(&bad), Err(Ok(Error::InvalidAmount)));
}

// -------------------------------------------------------------- the cap

#[test]
fn max_recipients_is_reported_to_clients() {
    let f = Fixture::new();
    assert_eq!(f.client.max_recipients(), MAX_RECIPIENTS);
}

#[test]
fn a_full_size_batch_executes() {
    let f = Fixture::new();
    let recipients = f.addresses(MAX_RECIPIENTS);
    let receipt = f
        .client
        .execute_equal(&f.funder, &f.token.address, &recipients, &10);
    assert_eq!(receipt.count, MAX_RECIPIENTS);
    assert_eq!(receipt.total, 10 * i128::from(MAX_RECIPIENTS));
}

/// Largest batch that executes under the test environment's budget.
///
/// Returns `None` if even the smallest probe fails. Resource exhaustion
/// surfaces as a host panic rather than a returnable error, so the probe has
/// to catch unwinds; the panic hook is silenced for the duration so the
/// expected failures do not fill the test log.
fn largest_batch_that_executes(candidates: &[u32]) -> Option<u32> {
    let previous = std::panic::take_hook();
    std::panic::set_hook(std::boxed::Box::new(|_| {}));

    let mut highest = None;
    for &n in candidates {
        let ok = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let env = Env::default();
            env.mock_all_auths();
            let funder = Address::generate(&env);
            let issuer = Address::generate(&env);
            let token = env.register_stellar_asset_contract_v2(issuer).address();
            StellarAssetClient::new(&env, &token).mint(&funder, &MINT);
            let client =
                BatchPayoutContractClient::new(&env, &env.register(BatchPayoutContract, ()));

            let mut recipients = Vec::new(&env);
            for _ in 0..n {
                recipients.push_back(Address::generate(&env));
            }
            // Call the entry point directly, bypassing MAX_RECIPIENTS, so the
            // probe measures resources rather than our own cap.
            client.execute_equal(&funder, &token, &recipients, &1);
        }))
        .is_ok();

        if ok {
            highest = Some(n);
        } else {
            break;
        }
    }

    std::panic::set_hook(previous);
    highest
}

#[test]
fn max_recipients_is_within_what_actually_executes() {
    // SPEC.md requires the cap be discovered empirically rather than guessed.
    // An earlier guess of 100 exceeded the budget, which is why this exists.
    let ceiling = largest_batch_that_executes(&[10, 20, 30, 40, 50, 60, 70, 80, 90, 100])
        .expect("even a 10-recipient batch failed to execute");
    std::println!("measured batch ceiling in the test environment: {ceiling} recipients");
    assert!(
        MAX_RECIPIENTS <= ceiling,
        "MAX_RECIPIENTS ({MAX_RECIPIENTS}) exceeds what executes ({ceiling})"
    );
}

#[test]
#[ignore = "reports the measured ceiling; run with --ignored --nocapture"]
fn report_batch_ceiling() {
    // Finer-grained sweep, for when the cap is being re-derived.
    //
    // Caveat that keeps MAX_RECIPIENTS conservative: the local test
    // environment models the CPU/memory budget but NOT the transaction size
    // limit a real network applies to the submitted envelope, and its
    // `mock_all_auths` builds a separate authorization entry per transfer
    // where a real submission signs one tree. So this measures a budget
    // ceiling, not the on-chain ceiling. Re-run against testnet before v0.2.
    let candidates: std::vec::Vec<u32> = (1..=20).map(|i| i * 5).collect();
    let ceiling = largest_batch_that_executes(&candidates);
    std::println!("measured batch ceiling in the test environment: {ceiling:?}");
}
