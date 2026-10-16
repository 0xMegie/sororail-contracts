// Test fixtures do plain arithmetic on known-small constants; the checked-math
// rule is for contract code.
#![allow(clippy::arithmetic_side_effects)]

use soroban_sdk::{
    testutils::{storage::Instance as _, Address as _, Events as _, Ledger as _},
    token::TokenClient,
    Address, Env, Event,
};
use sororail_common::{storage::INSTANCE_BUMP, testutils::TestEnv, Error};

use crate::{
    contract::{RecurringContract, RecurringContractClient},
    events::{Authorized, Cancelled, Charged},
};

#[test]
fn bump_restores_the_instance_ttl_after_ledger_advance() {
    let f = Fixture::new(None);
    f.env
        .ledger()
        .set_sequence_number(f.env.ledger().sequence() + INSTANCE_BUMP - 1);
    f.env.as_contract(&f.client.address, || {
        assert!(f.env.storage().instance().get_ttl() < INSTANCE_BUMP);
    });
    f.client.bump();
    f.env.as_contract(&f.client.address, || {
        assert_eq!(f.env.storage().instance().get_ttl(), INSTANCE_BUMP);
    });
}

const AMOUNT: i128 = 10_000;
const PERIOD: u64 = 2_592_000; // 30 days
const START: u64 = 1_000_000;
const MINT: i128 = AMOUNT * 100;

struct Fixture<'a> {
    env: Env,
    client: RecurringContractClient<'a>,
    token: TokenClient<'a>,
    payer: Address,
    payee: Address,
    outsider: Address,
}

impl Fixture<'_> {
    fn new(max_periods: Option<u32>) -> Self {
        let te = TestEnv::at(START);
        let payer = te.make_address();
        let payee = te.make_address();
        let outsider = te.make_address();
        let token_address = te.register_token();
        te.mint(&token_address, &payer, MINT);
        let env = te.env().clone();

        let client = RecurringContractClient::new(&env, &env.register(RecurringContract, ()));
        client.authorize(
            &payer,
            &payee,
            &token_address,
            &AMOUNT,
            &PERIOD,
            &max_periods,
        );

        let token = TokenClient::new(&env, &token_address);
        // The payer grants the contract an allowance -- without this, no charge
        // can move anything, however valid the authorization record is.
        let expiry = env.ledger().sequence() + 100_000;
        token.approve(&payer, &client.address, &MINT, &expiry);

        Fixture {
            token,
            env,
            client,
            payer,
            payee,
            outsider,
        }
    }

    fn at(&self, ts: u64) {
        self.env.ledger().with_mut(|l| l.timestamp = ts);
    }
}

// --------------------------------------------------------------- authorize

#[test]
fn authorize_schedules_the_first_charge_one_period_out() {
    let f = Fixture::new(None);
    assert_eq!(f.client.next_chargeable_at(), START + PERIOD);
    assert_eq!(f.client.get().periods_charged, 0);
    // Nothing has moved at signup.
    assert_eq!(f.token.balance(&f.payee), 0);
    assert!(!f.client.is_chargeable());
}

#[test]
fn authorize_rejects_a_non_positive_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let c = RecurringContractClient::new(&env, &env.register(RecurringContract, ()));
    let (a, b, t) = (
        Address::generate(&env),
        Address::generate(&env),
        Address::generate(&env),
    );
    for bad in [0i128, -1, i128::MIN] {
        assert_eq!(
            c.try_authorize(&a, &b, &t, &bad, &PERIOD, &None),
            Err(Ok(Error::InvalidAmount)),
            "amount {bad} was accepted"
        );
    }
}

#[test]
fn authorize_rejects_a_zero_period() {
    let env = Env::default();
    env.mock_all_auths();
    let c = RecurringContractClient::new(&env, &env.register(RecurringContract, ()));
    let (a, b, t) = (
        Address::generate(&env),
        Address::generate(&env),
        Address::generate(&env),
    );
    assert_eq!(
        c.try_authorize(&a, &b, &t, &AMOUNT, &0, &None),
        Err(Ok(Error::InvalidDuration))
    );
}

#[test]
fn authorize_rejects_a_zero_cap() {
    let env = Env::default();
    env.mock_all_auths();
    let c = RecurringContractClient::new(&env, &env.register(RecurringContract, ()));
    let (a, b, t) = (
        Address::generate(&env),
        Address::generate(&env),
        Address::generate(&env),
    );
    assert_eq!(
        c.try_authorize(&a, &b, &t, &AMOUNT, &PERIOD, &Some(0)),
        Err(Ok(Error::InvalidAmount))
    );
}

#[test]
fn authorize_rejects_identical_payer_and_payee() {
    let env = Env::default();
    env.mock_all_auths();
    let c = RecurringContractClient::new(&env, &env.register(RecurringContract, ()));
    let party = Address::generate(&env);
    let token = Address::generate(&env);
    assert_eq!(
        c.try_authorize(&party, &party, &token, &AMOUNT, &PERIOD, &None),
        Err(Ok(Error::IdenticalParties))
    );
    assert_eq!(c.try_get(), Err(Ok(Error::NotInitialized)));
}

#[test]
fn authorize_rejects_a_second_call() {
    let f = Fixture::new(None);
    assert_eq!(
        f.client.try_authorize(
            &f.payer,
            &f.payee,
            &f.token.address,
            &AMOUNT,
            &PERIOD,
            &None
        ),
        Err(Ok(Error::AlreadyInitialized))
    );
}

#[test]
fn entry_points_error_before_authorize() {
    let env = Env::default();
    env.mock_all_auths();
    let c = RecurringContractClient::new(&env, &env.register(RecurringContract, ()));
    let who = Address::generate(&env);
    assert_eq!(c.try_charge(), Err(Ok(Error::NotInitialized)));
    assert_eq!(c.try_cancel(&who), Err(Ok(Error::NotInitialized)));
    assert_eq!(c.try_next_chargeable_at(), Err(Ok(Error::NotInitialized)));
    assert_eq!(c.try_is_chargeable(), Err(Ok(Error::NotInitialized)));
}

#[test]
fn authorize_rejects_multiple_invalid_args_in_order() {
    let f = Fixture::new(None);

    // InvalidAmount comes before InvalidDuration
    let env = f.env.clone();
    let contract_id = env.register(RecurringContract, ());
    let client = RecurringContractClient::new(&env, &contract_id);
    assert_eq!(
        client.try_authorize(&f.payer, &f.payee, &f.token.address, &0, &0, &None),
        Err(Ok(Error::InvalidAmount))
    );

    // InvalidDuration comes before InvalidBasisPoints
    let contract_id2 = env.register(RecurringContract, ());
    let client2 = RecurringContractClient::new(&env, &contract_id2);
    assert_eq!(
        client2.try_authorize(
            &f.payer,
            &f.payee,
            &f.token.address,
            &AMOUNT,
            &0,
            &Some(20_000)
        ),
        Err(Ok(Error::InvalidDuration))
    );

    // The fixture is already initialized. AlreadyInitialized comes before
    // amount validation on a second call.
    assert_eq!(
        f.client
            .try_authorize(&f.payer, &f.payee, &f.token.address, &0, &PERIOD, &None),
        Err(Ok(Error::AlreadyInitialized))
    );
}

#[test]
#[should_panic]
fn authorize_requires_the_payers_authorization() {
    let env = Env::default();
    let c = RecurringContractClient::new(&env, &env.register(RecurringContract, ()));
    c.authorize(
        &Address::generate(&env),
        &Address::generate(&env),
        &Address::generate(&env),
        &AMOUNT,
        &PERIOD,
        &None,
    );
}

#[test]
fn authorize_emits_the_authorized_event() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = START);

    let payer = Address::generate(&env);
    let payee = Address::generate(&env);
    let issuer = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(issuer).address();

    let client = RecurringContractClient::new(&env, &env.register(RecurringContract, ()));
    client.authorize(&payer, &payee, &token, &AMOUNT, &PERIOD, &Some(12));

    let expected = Authorized {
        payer: payer.clone(),
        payee: payee.clone(),
        token: token.clone(),
        amount_per_period: AMOUNT,
        period_seconds: PERIOD,
        max_periods: Some(12),
        first_chargeable_at: START + PERIOD,
    };
    assert_eq!(
        env.events().all(),
        std::vec![expected.to_xdr(&env, &client.address)],
    );
}

// ------------------------------------------------------------------ charge

#[test]
fn charge_is_blocked_until_the_period_has_elapsed() {
    let f = Fixture::new(None);
    assert_eq!(
        f.client.try_charge(),
        Err(Ok(Error::RecurringPeriodNotElapsed))
    );
    f.at(START + PERIOD - 1);
    assert_eq!(
        f.client.try_charge(),
        Err(Ok(Error::RecurringPeriodNotElapsed))
    );
    assert_eq!(f.token.balance(&f.payee), 0);
}

#[test]
fn charge_moves_funds_from_payer_to_payee() {
    let f = Fixture::new(None);
    f.at(START + PERIOD);
    assert_eq!(f.client.charge(), AMOUNT);

    assert_eq!(f.token.balance(&f.payee), AMOUNT);
    assert_eq!(f.token.balance(&f.payer), MINT - AMOUNT);
    // The contract itself never holds anything.
    assert_eq!(f.token.balance(&f.client.address), 0);
}

/// Pins the wire shape indexers decode (SPEC.md `indexed_events`). The
/// expected value is spelled out via `events::Charged`, so renaming a topic
/// or a field fails here.
#[test]
fn charge_emits_the_charged_event() {
    let f = Fixture::new(None);
    f.at(START + PERIOD);
    f.client.charge();

    let expected = Charged {
        payee: f.payee.clone(),
        payer: f.payer.clone(),
        amount: AMOUNT,
        periods_charged: 1,
        next_chargeable_at: START + PERIOD * 2,
    };
    // The contract emits `authorized` at setup and `charged` here -- take the last one.
    let all = f.env.events().all().filter_by_contract(&f.client.address);
    let events = all.events();
    let last = events.last().expect("no events emitted");
    assert_eq!(last, &expected.to_xdr(&f.env, &f.client.address));
}

#[test]
fn charge_cannot_be_taken_twice_in_one_period() {
    let f = Fixture::new(None);
    f.at(START + PERIOD);
    f.client.charge();
    assert_eq!(
        f.client.try_charge(),
        Err(Ok(Error::RecurringPeriodNotElapsed))
    );
    assert_eq!(f.token.balance(&f.payee), AMOUNT);
}

#[test]
fn skipped_periods_are_forfeited_not_banked() {
    // The consumer-protection property. A payee who forgets for three periods
    // gets one charge when they remember, not three.
    let f = Fixture::new(None);
    f.at(START + PERIOD * 4);
    assert_eq!(f.client.charge(), AMOUNT);
    assert_eq!(f.token.balance(&f.payee), AMOUNT);

    // And the next charge is a full period from *now*, not from the old due
    // date -- so they cannot immediately claim the backlog either.
    assert_eq!(f.client.next_chargeable_at(), START + PERIOD * 5);
    assert_eq!(
        f.client.try_charge(),
        Err(Ok(Error::RecurringPeriodNotElapsed))
    );
}

#[test]
fn charging_on_schedule_bills_once_per_period() {
    let f = Fixture::new(None);
    for period in 1..=6u64 {
        f.at(START + PERIOD * period);
        f.client.charge();
    }
    assert_eq!(f.token.balance(&f.payee), AMOUNT * 6);
    assert_eq!(f.client.get().periods_charged, 6);
}

#[test]
fn charge_stops_at_the_cap() {
    let f = Fixture::new(Some(3));
    for period in 1..=3u64 {
        f.at(START + PERIOD * period);
        f.client.charge();
    }
    assert_eq!(f.client.remaining_periods(), Some(0));

    f.at(START + PERIOD * 4);
    assert_eq!(f.client.try_charge(), Err(Ok(Error::RecurringExhausted)));
    assert_eq!(f.token.balance(&f.payee), AMOUNT * 3);
    assert!(!f.client.is_chargeable());
}

#[test]
fn remaining_periods_counts_down() {
    let f = Fixture::new(Some(2));
    assert_eq!(f.client.remaining_periods(), Some(2));
    f.at(START + PERIOD);
    f.client.charge();
    assert_eq!(f.client.remaining_periods(), Some(1));
    f.at(START + PERIOD * 2);
    f.client.charge();
    assert_eq!(f.client.remaining_periods(), Some(0));
}

#[test]
fn an_uncapped_authorization_reports_no_remaining_count() {
    let f = Fixture::new(None);
    assert_eq!(f.client.remaining_periods(), None);
}

#[test]
fn charge_fails_once_the_payer_revokes_the_allowance() {
    // The allowance is the real cap: the payer can stop charges on the token
    // without touching this contract.
    let f = Fixture::new(None);
    let expiry = f.env.ledger().sequence() + 100_000;
    f.token.approve(&f.payer, &f.client.address, &0, &expiry);

    f.at(START + PERIOD);
    assert!(f.client.try_charge().is_err());
    assert_eq!(f.token.balance(&f.payee), 0);
}

#[test]
fn charge_fails_when_allowance_covers_fewer_periods_than_remain() {
    // The allowance on the token is the real cap. When the allowance is exactly
    // N periods' worth, charge N times, but charge N+1 fails while periods_charged
    // stays at N (rollback from the failed transfer_from).
    let f = Fixture::new(None);
    let expiry = f.env.ledger().sequence() + 100_000;
    let two_periods = AMOUNT * 2;
    f.token
        .approve(&f.payer, &f.client.address, &two_periods, &expiry);

    // First charge: succeeds.
    f.at(START + PERIOD);
    assert_eq!(f.client.charge(), AMOUNT);
    assert_eq!(f.client.get().periods_charged, 1);
    assert_eq!(f.token.balance(&f.payee), AMOUNT);

    // Second charge: succeeds, allowance exactly exhausted.
    f.at(START + PERIOD * 2);
    assert_eq!(f.client.charge(), AMOUNT);
    assert_eq!(f.client.get().periods_charged, 2);
    assert_eq!(f.token.balance(&f.payee), AMOUNT * 2);

    // Third charge: fails because allowance is exhausted.
    f.at(START + PERIOD * 3);
    assert!(f.client.try_charge().is_err());
    // periods_charged must remain 2 after the failed transfer_from.
    assert_eq!(f.client.get().periods_charged, 2);
}

/// `charge` bumps `periods_charged` before `transfer_from`. A failed token
/// call must roll that write back — `get()` still shows the pre-charge count.
#[test]
fn charge_reverts_cleanly_when_transfer_from_fails() {
    let f = Fixture::new(None);
    // Drain the payer so transfer_from fails even with a live allowance.
    let payer_bal = f.token.balance(&f.payer);
    f.token.transfer(&f.payer, &f.outsider, &payer_bal);

    f.at(START + PERIOD);
    assert!(f.client.try_charge().is_err());
    assert_eq!(f.client.get().periods_charged, 0);
    assert_eq!(f.token.balance(&f.payee), 0);
}

#[test]
#[should_panic]
fn charge_requires_the_payees_authorization() {
    let f = Fixture::new(None);
    f.at(START + PERIOD);
    f.env.set_auths(&[]);
    f.client.charge();
}

/// When the payer's token allowance to this contract is insufficient,
/// `charge` must fail cleanly — reverting the `periods_charged` /
/// `next_chargeable_at` state that was persisted just before the
/// `transfer_from` call (#100).
#[test]
fn charge_reverts_schedule_state_when_allowance_is_insufficient() {
    let f = Fixture::new(None);
    let expiry = f.env.ledger().sequence() + 100_000;
    // Set allowance to less than one period's worth.
    f.token
        .approve(&f.payer, &f.client.address, &(AMOUNT - 1), &expiry);

    f.at(START + PERIOD);
    assert!(f.client.try_charge().is_err());
    // Schedule state must be unchanged — the failed transfer_from rolled
    // back the optimistic write.
    assert_eq!(f.client.get().periods_charged, 0);
    assert_eq!(f.client.next_chargeable_at(), START + PERIOD);
    assert_eq!(f.token.balance(&f.payee), 0);
}

/// When the payer's token allowance has expired (TTL reached 0), `charge`
/// must fail and leave the schedule state untouched (#100).
#[test]
fn charge_reverts_schedule_state_when_allowance_is_expired() {
    let f = Fixture::new(None);
    // Grant an allowance that expires immediately.
    let current_seq = f.env.ledger().sequence();
    f.token
        .approve(&f.payer, &f.client.address, &MINT, &(current_seq + 1));
    // Advance the ledger past the allowance expiry.
    f.env
        .ledger()
        .with_mut(|l| l.sequence_number = current_seq + 100_000);

    f.at(START + PERIOD);
    assert!(f.client.try_charge().is_err());
    assert_eq!(f.client.get().periods_charged, 0);
    assert_eq!(f.token.balance(&f.payee), 0);
}

// ------------------------------------------------------------------ cancel

#[test]
fn the_payer_can_cancel() {
    let f = Fixture::new(None);
    f.client.cancel(&f.payer);
    assert!(f.client.get().cancelled);

    f.at(START + PERIOD);
    assert_eq!(f.client.try_charge(), Err(Ok(Error::RecurringCancelled)));
    assert_eq!(f.token.balance(&f.payee), 0);
}

/// Pins the wire shape indexers decode (SPEC.md `indexed_events`). The
/// expected value is spelled out via `events::Cancelled`, so renaming a
/// topic or a field fails here.
#[test]
fn cancel_emits_the_cancelled_event() {
    let f = Fixture::new(None);
    f.at(START + PERIOD);
    f.client.charge();
    f.at(START + PERIOD + 500);
    f.client.cancel(&f.payer);

    let expected = Cancelled {
        cancelled_by: f.payer.clone(),
        periods_charged: 1,
        cancelled_at: START + PERIOD + 500,
    };
    // The contract also emits `authorized` and `charged` earlier -- take the last event.
    let all = f.env.events().all().filter_by_contract(&f.client.address);
    let events = all.events();
    let last = events.last().expect("no events emitted");
    assert_eq!(last, &expected.to_xdr(&f.env, &f.client.address));
}

#[test]
fn the_payee_can_cancel() {
    let f = Fixture::new(None);
    f.client.cancel(&f.payee);
    assert!(f.client.get().cancelled);
}

#[test]
fn an_outsider_cannot_cancel() {
    let f = Fixture::new(None);
    assert_eq!(
        f.client.try_cancel(&f.outsider),
        Err(Ok(Error::Unauthorized))
    );
    assert!(!f.client.get().cancelled);
}

#[test]
fn cancel_rejects_a_second_call() {
    let f = Fixture::new(None);
    f.client.cancel(&f.payer);
    assert_eq!(
        f.client.try_cancel(&f.payee),
        Err(Ok(Error::RecurringCancelled))
    );
}

#[test]
fn cancelling_mid_subscription_keeps_what_was_already_charged() {
    let f = Fixture::new(None);
    f.at(START + PERIOD);
    f.client.charge();
    f.client.cancel(&f.payer);

    assert_eq!(f.token.balance(&f.payee), AMOUNT);
    f.at(START + PERIOD * 2);
    assert_eq!(f.client.try_charge(), Err(Ok(Error::RecurringCancelled)));
    assert_eq!(f.token.balance(&f.payee), AMOUNT);
}

#[test]
fn is_chargeable_tracks_the_schedule_and_the_cancellation() {
    let f = Fixture::new(None);
    assert!(!f.client.is_chargeable());
    f.at(START + PERIOD);
    assert!(f.client.is_chargeable());
    f.client.cancel(&f.payer);
    assert!(!f.client.is_chargeable());
}
