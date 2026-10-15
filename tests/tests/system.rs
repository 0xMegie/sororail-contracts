//! Cross-contract integration tests.
//!
//! Each contract's own unit tests already prove it conserves value internally.
//! What they cannot see is the system: several SoroRail contracts holding the
//! same token at once, funded by the same account, unwinding in different
//! orders. These tests deploy real Stellar Asset Contract tokens and run full
//! lifecycles across all five contracts together.
//!
//! The invariant under test is the one that matters to an operator closing the
//! books: **no token is created or destroyed anywhere in the system.** Every
//! test ends by summing every balance — every party and every contract
//! address — and asserting it equals what was minted.

#![allow(clippy::arithmetic_side_effects)]

use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token::{StellarAssetClient, TokenClient},
    vec, Address, Env, Vec,
};
use sororail_batch_payout::{BatchPayoutContract, BatchPayoutContractClient, Payment};
use sororail_escrow::{EscrowContract, EscrowContractClient};
use sororail_recurring::{RecurringContract, RecurringContractClient};
use sororail_stream::{StreamContract, StreamContractClient};
use sororail_vesting::{VestingContract, VestingContractClient};

const START: u64 = 1_000_000;
const MINT: i128 = 100_000_000;
const YEAR: u64 = 31_536_000;
const MONTH: u64 = 2_592_000;

/// One employer, one token, and every SoroRail contract deployed against it.
struct System<'a> {
    env: Env,
    token: TokenClient<'a>,
    employer: Address,
    escrow: EscrowContractClient<'a>,
    stream: StreamContractClient<'a>,
    vesting: VestingContractClient<'a>,
    recurring: RecurringContractClient<'a>,
    batch: BatchPayoutContractClient<'a>,
}

impl System<'_> {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = START);

        let employer = Address::generate(&env);
        let issuer = Address::generate(&env);
        let token_address = env.register_stellar_asset_contract_v2(issuer).address();
        StellarAssetClient::new(&env, &token_address).mint(&employer, &MINT);

        System {
            token: TokenClient::new(&env, &token_address),
            escrow: EscrowContractClient::new(&env, &env.register(EscrowContract, ())),
            stream: StreamContractClient::new(&env, &env.register(StreamContract, ())),
            vesting: VestingContractClient::new(&env, &env.register(VestingContract, ())),
            recurring: RecurringContractClient::new(&env, &env.register(RecurringContract, ())),
            batch: BatchPayoutContractClient::new(&env, &env.register(BatchPayoutContract, ())),
            employer,
            env,
        }
    }

    fn at(&self, ts: u64) {
        self.env.ledger().with_mut(|l| l.timestamp = ts);
    }

    fn balance(&self, who: &Address) -> i128 {
        self.token.balance(who)
    }

    /// Every address the system can hold value at: the four stateful contracts
    /// plus whoever the test names. `batch_payout` is stateless and never
    /// holds a balance, but it is included so that would be caught if it did.
    fn total_held(&self, parties: &[&Address]) -> i128 {
        let mut sum = self.balance(&self.employer);
        for p in parties {
            sum += self.balance(p);
        }
        for contract in [
            &self.escrow.address,
            &self.stream.address,
            &self.vesting.address,
            &self.recurring.address,
            &self.batch.address,
        ] {
            sum += self.balance(contract);
        }
        sum
    }

    /// Nothing has been created or destroyed anywhere in the system.
    fn assert_nothing_lost(&self, parties: &[&Address]) {
        assert_eq!(
            self.total_held(parties),
            MINT,
            "value leaked or was conjured somewhere in the system"
        );
    }
}

/// A month of operations touching all five contracts, then unwound.
#[test]
fn an_operations_month_conserves_every_token() {
    let s = System::new();

    let alice = Address::generate(&s.env); // contractor, paid by batch
    let bob = Address::generate(&s.env); // contractor, paid by batch
    let carol = Address::generate(&s.env); // employee, on a vesting grant
    let dave = Address::generate(&s.env); // contractor, on a stream
    let supplier = Address::generate(&s.env); // escrow beneficiary
    let arbiter = Address::generate(&s.env);
    let vendor = Address::generate(&s.env); // SaaS, on a subscription
    let parties = [&alice, &bob, &carol, &dave, &supplier, &arbiter, &vendor];

    // --- payday: two contractors, one transaction ---
    let payroll = vec![
        &s.env,
        Payment {
            to: alice.clone(),
            amount: 500_000,
        },
        Payment {
            to: bob.clone(),
            amount: 750_000,
        },
    ];
    let receipt = s.batch.execute(&s.employer, &s.token.address, &payroll);
    assert_eq!(receipt.total, 1_250_000);
    s.assert_nothing_lost(&parties);

    // --- a four-year grant for the employee, one-year cliff ---
    s.vesting.create(
        &s.employer,
        &carol,
        &s.token.address,
        &12_000_000,
        &START,
        &YEAR,
        &(YEAR * 4),
        &true,
    );
    s.assert_nothing_lost(&parties);

    // --- a stream for the contractor, one year at 1/sec ---
    s.stream.create(
        &s.employer,
        &dave,
        &s.token.address,
        &1,
        &START,
        &(START + YEAR),
        &true,
    );
    s.assert_nothing_lost(&parties);

    // --- an escrow against a supplier delivery, with an arbiter ---
    s.escrow.init(
        &s.employer,
        &supplier,
        &Some(arbiter.clone()),
        &s.token.address,
        &2_000_000,
        &(START + MONTH * 2),
    );
    s.escrow.fund();
    s.assert_nothing_lost(&parties);

    // --- a monthly subscription to the vendor ---
    s.recurring.authorize(
        &s.employer,
        &vendor,
        &s.token.address,
        &99_000,
        &MONTH,
        &Some(12),
    );
    let expiry = s.env.ledger().sequence() + 500_000;
    s.token
        .approve(&s.employer, &s.recurring.address, &MINT, &expiry);
    s.assert_nothing_lost(&parties);

    // --- a month passes ---
    s.at(START + MONTH);
    s.recurring.charge();
    s.stream.withdraw(&None);
    s.assert_nothing_lost(&parties);
    assert_eq!(s.balance(&vendor), 99_000);
    assert_eq!(s.balance(&dave), i128::from(MONTH));

    // The grant is still inside its cliff.
    assert_eq!(s.vesting.claimable(), 0);

    // --- the supplier delivers; the escrow is released ---
    s.escrow.release(&s.employer);
    assert_eq!(s.balance(&supplier), 2_000_000);
    s.assert_nothing_lost(&parties);

    // --- eighteen months on: the cliff has passed, the stream has ended ---
    s.at(START + YEAR + MONTH * 6);
    s.vesting.claim();
    s.stream.withdraw(&None);
    s.assert_nothing_lost(&parties);

    // The stream paid out its full year and no more.
    assert_eq!(s.balance(&dave), i128::from(YEAR));
    assert_eq!(s.stream.remaining(), 0);

    // --- the employee leaves; the unvested remainder comes back ---
    let returned = s.vesting.revoke();
    assert!(returned > 0, "an unvested remainder should have returned");
    s.assert_nothing_lost(&parties);

    // --- and the subscription is cancelled ---
    s.recurring.cancel(&s.employer);
    s.assert_nothing_lost(&parties);
}

/// Two contracts holding the same token must not see each other's funds.
#[test]
fn contracts_sharing_a_token_stay_isolated() {
    let s = System::new();
    let recipient = Address::generate(&s.env);
    let beneficiary = Address::generate(&s.env);
    let parties = [&recipient, &beneficiary];

    s.stream.create(
        &s.employer,
        &recipient,
        &s.token.address,
        &10,
        &START,
        &(START + 1_000),
        &true,
    );
    s.escrow.init(
        &s.employer,
        &beneficiary,
        &None,
        &s.token.address,
        &50_000,
        &(START + 10_000),
    );
    s.escrow.fund();

    // Each contract holds exactly its own commitment.
    assert_eq!(s.balance(&s.stream.address), 10_000);
    assert_eq!(s.balance(&s.escrow.address), 50_000);

    // Draining the stream in full leaves the escrow untouched.
    s.at(START + 1_000);
    s.stream.withdraw(&None);
    assert_eq!(s.balance(&s.stream.address), 0);
    assert_eq!(s.balance(&s.escrow.address), 50_000);
    assert_eq!(s.balance(&recipient), 10_000);
    assert_eq!(s.balance(&beneficiary), 0);
    s.assert_nothing_lost(&parties);

    // And releasing the escrow leaves the spent stream at zero.
    s.escrow.release(&s.employer);
    assert_eq!(s.balance(&beneficiary), 50_000);
    assert_eq!(s.balance(&s.stream.address), 0);
    s.assert_nothing_lost(&parties);
}

/// Attempting to cancel a non-cancellable stream should fail.
#[test]
fn cannot_cancel_non_cancellable_stream() {
    let s = System::new();
    let recipient = Address::generate(&s.env);
    let parties = [&recipient];

    // Create a non-cancellable stream
    s.stream.create(
        &s.employer,
        &recipient,
        &s.token.address,
        &10,
        &START,
        &(START + 1_000),
        &false, // Not cancellable
    );
    s.assert_nothing_lost(&parties);

    // Attempt to cancel the stream and assert the error
    let result = s.stream.try_cancel();
    assert_eq!(
        result,
        Err(Ok(sororail_common::Error::StreamNotCancellable))
    );
    s.assert_nothing_lost(&parties);
}

/// Cancelling and revoking in the same window returns exactly the unearned
/// portion of each, and nothing more.
#[test]
fn simultaneous_unwinds_return_only_what_was_unearned() {
    let s = System::new();
    let streamer = Address::generate(&s.env);
    let grantee = Address::generate(&s.env);
    let parties = [&streamer, &grantee];

    // 1000 seconds at 100/sec, and a grant of 1_000_000 over 1000s, no cliff.
    s.stream.create(
        &s.employer,
        &streamer,
        &s.token.address,
        &100,
        &START,
        &(START + 1_000),
        &true,
    );
    s.vesting.create(
        &s.employer,
        &grantee,
        &s.token.address,
        &1_000_000,
        &START,
        &0,
        &1_000,
        &true,
    );
    let committed = 100_000 + 1_000_000;
    assert_eq!(s.balance(&s.employer), MINT - committed);

    // A quarter of the way through, unwind both.
    s.at(START + 250);
    s.stream.cancel();
    let returned = s.vesting.revoke();

    // The stream settled 25% to the recipient and refunded 75%.
    assert_eq!(s.balance(&streamer), 25_000);
    // The grant returned 75% and left 25% claimable in the contract.
    assert_eq!(returned, 750_000);
    assert_eq!(s.vesting.claimable(), 250_000);

    s.assert_nothing_lost(&parties);

    // The grantee collects the vested quarter; the books close at zero.
    s.vesting.claim();
    assert_eq!(s.balance(&grantee), 250_000);
    assert_eq!(s.balance(&s.stream.address), 0);
    assert_eq!(s.balance(&s.vesting.address), 0);
    assert_eq!(s.balance(&s.employer), MINT - 25_000 - 250_000);
    s.assert_nothing_lost(&parties);
}

/// A batch payout to people who are also stream recipients credits both.
#[test]
fn a_recipient_can_be_paid_by_two_contracts_at_once() {
    let s = System::new();
    let worker = Address::generate(&s.env);
    let parties = [&worker];

    s.stream.create(
        &s.employer,
        &worker,
        &s.token.address,
        &5,
        &START,
        &(START + 1_000),
        &false,
    );

    let bonus = vec![
        &s.env,
        Payment {
            to: worker.clone(),
            amount: 40_000,
        },
    ];
    s.batch.execute(&s.employer, &s.token.address, &bonus);
    assert_eq!(s.balance(&worker), 40_000);

    s.at(START + 1_000);
    s.stream.withdraw(&None);
    assert_eq!(s.balance(&worker), 40_000 + 5_000);
    s.assert_nothing_lost(&parties);
}

/// A batch at the documented cap runs alongside the other contracts.
#[test]
fn a_full_size_batch_runs_within_a_populated_system() {
    let s = System::new();

    // Something else is already holding funds.
    let beneficiary = Address::generate(&s.env);
    s.escrow.init(
        &s.employer,
        &beneficiary,
        &None,
        &s.token.address,
        &1_000,
        &(START + 10_000),
    );
    s.escrow.fund();

    let mut recipients = Vec::new(&s.env);
    for _ in 0..sororail_batch_payout::MAX_RECIPIENTS {
        recipients.push_back(Address::generate(&s.env));
    }
    let receipt = s
        .batch
        .execute_equal(&s.employer, &s.token.address, &recipients, &100);

    assert_eq!(receipt.count, sororail_batch_payout::MAX_RECIPIENTS);
    for r in recipients.iter() {
        assert_eq!(s.balance(&r), 100);
    }

    let paid_out: i128 = 100 * i128::from(sororail_batch_payout::MAX_RECIPIENTS);
    assert_eq!(s.balance(&s.employer), MINT - paid_out - 1_000);
}

/// Payroll workflow combining batch_payout for immediate multi-recipient payments
/// and recurring for ongoing vendor subscriptions. Both operate on the same token
/// and employer account, demonstrating the "Payroll (batch_payout + recurring)"
/// feature from SPEC.md that uses both contracts in a real workflow.
#[test]
fn batch_and_recurring_payroll_workflow() {
    let s = System::new();

    let contractor_a = Address::generate(&s.env);
    let contractor_b = Address::generate(&s.env);
    let vendor = Address::generate(&s.env);
    let parties = [&contractor_a, &contractor_b, &vendor];

    // --- Immediate payroll via batch: pay multiple contractors at once ---
    let payroll = vec![
        &s.env,
        Payment {
            to: contractor_a.clone(),
            amount: 1_000_000,
        },
        Payment {
            to: contractor_b.clone(),
            amount: 750_000,
        },
    ];
    let receipt = s.batch.execute(&s.employer, &s.token.address, &payroll);
    assert_eq!(receipt.total, 1_750_000);
    s.assert_nothing_lost(&parties);

    // --- Recurring subscription: employer authorizes vendor for monthly charges ---
    // Same employer funding both the batch payroll and the vendor subscription
    s.recurring.authorize(
        &s.employer,
        &vendor,
        &s.token.address,
        &100_000,
        &MONTH,
        &Some(12),
    );
    let expiry = s.env.ledger().sequence() + 500_000;
    s.token
        .approve(&s.employer, &s.recurring.address, &MINT, &expiry);
    s.assert_nothing_lost(&parties);

    // --- First month: vendor pulls subscription charge ---
    s.at(START + MONTH);
    s.recurring.charge();
    assert_eq!(s.balance(&vendor), 100_000);
    s.assert_nothing_lost(&parties);

    // --- Second month: another batch payout for contractors, vendor charges again ---
    s.at(START + MONTH * 2);
    let bonus_payroll = vec![
        &s.env,
        Payment {
            to: contractor_a.clone(),
            amount: 250_000,
        },
        Payment {
            to: contractor_b.clone(),
            amount: 200_000,
        },
    ];
    s.batch
        .execute(&s.employer, &s.token.address, &bonus_payroll);
    s.recurring.charge();
    assert_eq!(s.balance(&vendor), 200_000);
    s.assert_nothing_lost(&parties);

    // Verify total conservation across batch and recurring contracts and parties
    assert_eq!(
        s.balance(&contractor_a)
            + s.balance(&contractor_b)
            + s.balance(&vendor)
            + s.balance(&s.employer)
            + s.balance(&s.batch.address)
            + s.balance(&s.recurring.address),
        MINT
    );
}
