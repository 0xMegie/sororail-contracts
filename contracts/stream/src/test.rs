// Test fixtures do plain arithmetic on known-small constants; the checked-math
// rule is for contract code.
#![allow(clippy::arithmetic_side_effects)]

use soroban_sdk::{
    map,
    testutils::{storage::Instance as _, Address as _, Events as _, Ledger as _},
    token::TokenClient,
    vec, Address, Env, IntoVal, Symbol, Val,
};
use sororail_common::{storage::INSTANCE_BUMP, testutils::TestEnv, Error};

use crate::{
    contract::{StreamContract, StreamContractClient},
    events::{Cancelled, Created, Withdrawn},
    types::Stream,
};

#[test]
fn bump_restores_the_instance_ttl_after_ledger_advance() {
    let f = Fixture::new();
    f.env
        .ledger()
        .set_sequence_number(f.env.ledger().sequence() + INSTANCE_BUMP - 1);
    assert!(f.env.storage().instance().get_ttl() < INSTANCE_BUMP);
    f.client.bump();
    assert_eq!(f.env.storage().instance().get_ttl(), INSTANCE_BUMP);
}

const RATE: i128 = 100;
const START: u64 = 1_000_000;
const DURATION: u64 = 1_000;
const STOP: u64 = START + DURATION;
const DEPOSITED: i128 = RATE * DURATION as i128;
const MINT: i128 = DEPOSITED * 10;

struct Fixture<'a> {
    env: Env,
    client: StreamContractClient<'a>,
    token: TokenClient<'a>,
    sender: Address,
    recipient: Address,
    outsider: Address,
}

impl Fixture<'_> {
    fn new(cancellable: bool) -> Self {
        let te = TestEnv::at(START);
        let (token, sender) = te.make_token(MINT);
        let token_address = token.address.clone();
        let recipient = te.make_address();
        let outsider = te.make_address();
        let env = te.env;

        let client = StreamContractClient::new(&env, &env.register(StreamContract, ()));
        client.create(
            &sender,
            &recipient,
            &token_address,
            &RATE,
            &START,
            &STOP,
            &cancellable,
        );

        Fixture {
            token: TokenClient::new(&env, &token_address),
            env,
            client,
            sender,
            recipient,
            outsider,
        }
    }

    fn at(&self, ts: u64) {
        self.env.ledger().with_mut(|l| l.timestamp = ts);
    }

    fn held(&self) -> i128 {
        self.token.balance(&self.client.address)
    }

    /// The property the whole contract exists to preserve.
    fn assert_conserved(&self) {
        let s = self.client.get();
        assert_eq!(
            s.withdrawn + s.refunded + self.client.remaining(),
            s.deposited,
            "conservation violated: {s:?}"
        );
        assert_eq!(
            self.held(),
            self.client.remaining(),
            "held balance diverged from remaining: {s:?}"
        );
    }
}

/// A bare struct for testing accrual in isolation, with no contract or ledger.
fn bare(env: &Env) -> Stream {
    Stream {
        sender: Address::generate(env),
        recipient: Address::generate(env),
        token: Address::generate(env),
        rate_per_second: RATE,
        start: START,
        stop: STOP,
        cancellable: true,
        deposited: DEPOSITED,
        withdrawn: 0,
        refunded: 0,
        cancelled_at: None,
    }
}

// ------------------------------------------------ accrual (pure, no ledger)

#[test]
fn nothing_accrues_before_start() {
    let env = Env::default();
    let s = bare(&env);
    assert_eq!(s.accrued_at(0), Ok(0));
    assert_eq!(s.accrued_at(START - 1), Ok(0));
    assert_eq!(s.accrued_at(START), Ok(0));
}

#[test]
fn accrual_is_linear_between_start_and_stop() {
    let env = Env::default();
    let s = bare(&env);
    assert_eq!(s.accrued_at(START + 1), Ok(RATE));
    assert_eq!(s.accrued_at(START + 250), Ok(RATE * 250));
    assert_eq!(s.accrued_at(START + DURATION / 2), Ok(DEPOSITED / 2));
}

#[test]
fn accrual_is_capped_at_stop() {
    let env = Env::default();
    let s = bare(&env);
    assert_eq!(s.accrued_at(STOP), Ok(DEPOSITED));
    assert_eq!(s.accrued_at(STOP + 1), Ok(DEPOSITED));
    assert_eq!(s.accrued_at(u64::MAX), Ok(DEPOSITED));
}

#[test]
fn accrual_halts_at_cancellation() {
    let env = Env::default();
    let mut s = bare(&env);
    s.cancelled_at = Some(START + 100);
    assert_eq!(s.accrued_at(START + 100), Ok(RATE * 100));
    // Time moving on does not accrue any more.
    assert_eq!(s.accrued_at(START + 500), Ok(RATE * 100));
    assert_eq!(s.accrued_at(STOP + 5_000), Ok(RATE * 100));
}

#[test]
fn a_one_second_stream_accrues_exactly_one_tick() {
    let env = Env::default();
    let mut s = bare(&env);
    s.stop = START + 1;
    s.deposited = RATE;
    assert_eq!(s.accrued_at(START), Ok(0));
    assert_eq!(s.accrued_at(START + 1), Ok(RATE));
    assert_eq!(s.accrued_at(START + 2), Ok(RATE));
}

#[test]
fn accrual_reports_overflow_rather_than_wrapping() {
    let env = Env::default();
    let mut s = bare(&env);
    s.rate_per_second = i128::MAX;
    s.deposited = i128::MAX;
    assert_eq!(s.accrued_at(START + 2), Err(Error::Overflow));
}

/// Documents that `available_at` degrades to `Err(Underflow)` if `withdrawn`
/// ever exceeds accrued (e.g. a future code path bug). Today's `withdraw`
/// rejects `requested > available`, so this state is unreachable through the
/// contract entry points; the pure helper must still not wrap to a negative
/// `i128` if it is reached.
#[test]
fn available_at_errors_if_withdrawn_exceeds_accrued() {
    let env = Env::default();
    let mut s = bare(&env);
    // Accrued at START+10 is RATE*10; force withdrawn past that.
    s.withdrawn = RATE * 10 + 1;
    assert_eq!(
        s.available_at(START + 10),
        Err(Error::Underflow),
        "available_at must error rather than wrap when withdrawn > accrued"
    );
    // Even after full accrual, an over-withdrawn book still errors.
    s.withdrawn = DEPOSITED + 1;
    assert_eq!(s.available_at(STOP), Err(Error::Underflow));
}

// ------------------------------------------------------------------ create

#[test]
fn create_funds_the_whole_span_up_front() {
    let f = Fixture::new(true);
    assert_eq!(f.held(), DEPOSITED);
    assert_eq!(f.token.balance(&f.sender), MINT - DEPOSITED);
    let s = f.client.get();
    assert_eq!(s.deposited, DEPOSITED);
    assert_eq!(s.withdrawn, 0);
    assert_eq!(s.refunded, 0);
    assert_eq!(s.cancelled_at, None);
    f.assert_conserved();
}

/// Pins Created topics and data fields (SPEC.md `indexed_events`).
#[test]
fn create_emits_the_created_event() {
    let f = Fixture::new(true);
    let events = f.env.events().all();
    assert_eq!(events.len(), 1);
    let expected = Created {
        sender: f.sender.clone(),
        recipient: f.recipient.clone(),
        token: f.token.address.clone(),
        rate_per_second: RATE,
        start: START,
        stop: STOP,
        cancellable: true,
        deposited: DEPOSITED,
    };
    assert_eq!(&events[0], &expected);
}

#[test]
fn create_rejects_a_non_positive_rate() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = START);
    let c = StreamContractClient::new(&env, &env.register(StreamContract, ()));
    let (a, b, t) = (
        Address::generate(&env),
        Address::generate(&env),
        Address::generate(&env),
    );
    for bad in [0i128, -1, i128::MIN] {
        assert_eq!(
            c.try_create(&a, &b, &t, &bad, &START, &STOP, &true),
            Err(Ok(Error::InvalidAmount)),
            "rate {bad} was accepted"
        );
    }
}

#[test]
fn create_rejects_an_empty_or_inverted_span() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = START);
    let c = StreamContractClient::new(&env, &env.register(StreamContract, ()));
    let (a, b, t) = (
        Address::generate(&env),
        Address::generate(&env),
        Address::generate(&env),
    );
    for bad_stop in [START, START - 1, 0] {
        assert_eq!(
            c.try_create(&a, &b, &t, &RATE, &START, &bad_stop, &true),
            Err(Ok(Error::InvalidTimeRange)),
            "stop {bad_stop} was accepted"
        );
    }
}

#[test]
fn create_rejects_funding_that_overflows() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = START);
    let c = StreamContractClient::new(&env, &env.register(StreamContract, ()));
    let (a, b, t) = (
        Address::generate(&env),
        Address::generate(&env),
        Address::generate(&env),
    );
    assert_eq!(
        c.try_create(&a, &b, &t, &i128::MAX, &START, &(START + 2), &true),
        Err(Ok(Error::Overflow))
    );
}

#[test]
fn create_allows_the_exact_max_funding_boundary() {
    // `rate_per_second * (stop - start) == i128::MAX` is the largest a stream
    // can be funded: it must succeed. `i128::MAX` is prime (Mersenne prime
    // M127), so the only single-second-span way to land exactly on it is
    // `rate = i128::MAX`, `duration = 1`; one more second overflows (asserted
    // just above, in `create_rejects_funding_that_overflows`).
    let te = TestEnv::at(START);
    let (token_client, sender) = te.make_token(i128::MAX);
    let token = token_client.address.clone();
    let recipient = te.make_address();
    let env = te.env;
    let c = StreamContractClient::new(&env, &env.register(StreamContract, ()));

    c.create(
        &sender,
        &recipient,
        &token,
        &i128::MAX,
        &START,
        &(START + 1),
        &true,
    );

    let s = c.get();
    assert_eq!(s.rate_per_second, i128::MAX);
    assert_eq!(s.stop - s.start, 1);
    assert_eq!(s.deposited, i128::MAX);
    assert_eq!(s.withdrawn, 0);
    assert_eq!(c.remaining(), i128::MAX);
    assert_eq!(token_client.balance(&c.address), i128::MAX);
    assert_eq!(token_client.balance(&sender), 0);
}

#[test]
fn create_rejects_a_second_call() {
    let f = Fixture::new(true);
    assert_eq!(
        f.client.try_create(
            &f.sender,
            &f.recipient,
            &f.token.address,
            &RATE,
            &START,
            &STOP,
            &true
        ),
        Err(Ok(Error::AlreadyInitialized))
    );
}

#[test]
fn create_rejects_identical_sender_and_recipient() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = START);
    let c = StreamContractClient::new(&env, &env.register(StreamContract, ()));
    let party = Address::generate(&env);
    let token = Address::generate(&env);
    assert_eq!(
        c.try_create(&party, &party, &token, &RATE, &START, &STOP, &true),
        Err(Ok(Error::IdenticalParties))
    );
}

/// A self-stream is refused before any funds move, and leaves no stream behind.
#[test]
fn self_stream_is_refused_without_moving_funds() {
    let te = TestEnv::at(START);
    let (token, sender) = te.make_token(MINT);
    let env = te.env;
    let client = StreamContractClient::new(&env, &env.register(StreamContract, ()));
    assert_eq!(
        client.try_create(&sender, &sender, &token.address, &RATE, &START, &STOP, &true),
        Err(Ok(Error::IdenticalParties))
    );
    assert_eq!(token.balance(&sender), MINT);
    assert_eq!(token.balance(&client.address), 0);
    assert_eq!(client.try_get(), Err(Ok(Error::NotInitialized)));
}

/// `create` saves the stream before pulling funds. If the token transfer
/// fails, the host reverts the whole invocation — including that save — so a
/// later `get` still sees `NotInitialized`.
#[test]
fn create_reverts_cleanly_when_sender_cannot_fund() {
    let te = TestEnv::at(START);
    // Mint far less than the deposit the span requires.
    let (token_client, sender) = te.make_token(RATE);
    let token = token_client.address.clone();
    let recipient = te.make_address();
    let env = te.env;
    let c = StreamContractClient::new(&env, &env.register(StreamContract, ()));

    assert!(c
        .try_create(&sender, &recipient, &token, &RATE, &START, &STOP, &true)
        .is_err());
    assert_eq!(c.try_get(), Err(Ok(Error::NotInitialized)));
    assert_eq!(token_client.balance(&c.address), 0);
    assert_eq!(token_client.balance(&sender), RATE);
}
#[test]
fn entry_points_error_before_create() {
    let env = Env::default();
    env.mock_all_auths();
    let c = StreamContractClient::new(&env, &env.register(StreamContract, ()));
    let who = Address::generate(&env);
    assert_eq!(c.try_withdraw(&None), Err(Ok(Error::NotInitialized)));
    assert_eq!(c.try_cancel(), Err(Ok(Error::NotInitialized)));
    assert_eq!(c.try_top_up(&RATE), Err(Ok(Error::NotInitialized)));
    assert_eq!(c.try_extend(&STOP), Err(Ok(Error::NotInitialized)));
    assert_eq!(c.try_balance_of(&who), Err(Ok(Error::NotInitialized)));
    assert_eq!(c.try_remaining(), Err(Ok(Error::NotInitialized)));
}

#[test]
fn create_with_contract_as_recipient_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = START);
    let sender = Address::generate(&env);
    let issuer = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(issuer).address();
    let contract_id = env.register(StreamContract, ());
    let client = StreamContractClient::new(&env, &contract_id);

    StellarAssetClient::new(&env, &token).mint(&sender, &(RATE * 1000));

    let result = client.try_create(&sender, &contract_id, &token, &RATE, &START, &STOP, &true);
    assert!(
        result.is_ok(),
        "create with contract as recipient should succeed"
    );
}

#[test]
fn create_rejects_multiple_invalid_args_in_order() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = START);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    let issuer = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(issuer).address();
    let contract_id = env.register(StreamContract, ());
    let client = StreamContractClient::new(&env, &contract_id);

    // AlreadyInitialized comes before InvalidTimeRange
    client.create(&sender, &recipient, &token, &RATE, &START, &STOP, &true);
    assert_eq!(
        client.try_create(&sender, &recipient, &token, &RATE, &START, &START, &true),
        Err(Ok(Error::AlreadyInitialized))
    );

    // IdenticalParties check comes before rate validation
    let contract_id2 = env.register(StreamContract, ());
    let client2 = StreamContractClient::new(&env, &contract_id2);
    assert_eq!(
        client2.try_create(&sender, &sender, &token, &0, &START, &STOP, &true),
        Err(Ok(Error::IdenticalParties))
    );

    // InvalidTimeRange check comes before overflow check
    let contract_id3 = env.register(StreamContract, ());
    let client3 = StreamContractClient::new(&env, &contract_id3);
    assert_eq!(
        client3.try_create(&sender, &recipient, &token, &RATE, &STOP, &START, &true),
        Err(Ok(Error::InvalidTimeRange))
    );
}

#[test]
#[should_panic]
fn create_requires_the_senders_authorization() {
    let env = Env::default();
    env.ledger().with_mut(|l| l.timestamp = START);
    let c = StreamContractClient::new(&env, &env.register(StreamContract, ()));
    c.create(
        &Address::generate(&env),
        &Address::generate(&env),
        &Address::generate(&env),
        &RATE,
        &START,
        &STOP,
        &true,
    );
}

// ---------------------------------------------------------------- withdraw

#[test]
fn withdraw_none_takes_everything_available() {
    let f = Fixture::new(true);
    f.at(START + 300);
    assert_eq!(f.client.withdraw(&None), RATE * 300);
    assert_eq!(f.token.balance(&f.recipient), RATE * 300);
    f.assert_conserved();
}

#[test]
fn withdraw_emits_the_withdrawn_event() {
    let f = Fixture::new(true);
    f.at(START + 300);
    f.client.withdraw(&None);

    let events = f.env.events().all();
    // Created + Withdrawn
    assert_eq!(events.len(), 2);
    let expected = Withdrawn {
        recipient: f.recipient.clone(),
        amount: RATE * 300,
        total_withdrawn: RATE * 300,
    };
    assert_eq!(&events[1], &expected);
}

#[test]
fn withdraw_takes_a_partial_amount() {
    let f = Fixture::new(true);
    f.at(START + 300);
    assert_eq!(f.client.withdraw(&Some(RATE * 100)), RATE * 100);
    assert_eq!(f.client.balance_of(&f.recipient), RATE * 200);
    f.assert_conserved();
}

#[test]
fn withdraw_rejects_more_than_has_accrued() {
    let f = Fixture::new(true);
    f.at(START + 300);
    assert_eq!(
        f.client.try_withdraw(&Some(RATE * 301)),
        Err(Ok(Error::StreamInsufficientAccrued))
    );
    assert_eq!(f.held(), DEPOSITED);
}

#[test]
fn withdraw_rejects_when_nothing_has_accrued() {
    let f = Fixture::new(true);
    // Still at `start`; nothing accrued.
    assert_eq!(f.client.try_withdraw(&None), Err(Ok(Error::InvalidAmount)));
    assert_eq!(
        f.client.try_withdraw(&Some(0)),
        Err(Ok(Error::InvalidAmount))
    );
    assert_eq!(
        f.client.try_withdraw(&Some(-1)),
        Err(Ok(Error::InvalidAmount))
    );
}

#[test]
fn withdraw_some_zero_distinctly_from_none_with_zero_accrued() {
    let f = Fixture::new(true);
    // Explicit Some(0) flows through unwrap_or untouched into require_positive.
    // Both None and Some(0) should fail with InvalidAmount when nothing accrued.
    let none_result = f.client.try_withdraw(&None);
    let some_zero_result = f.client.try_withdraw(&Some(0));
    assert_eq!(none_result, Err(Ok(Error::InvalidAmount)));
    assert_eq!(some_zero_result, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn withdrawing_the_whole_stream_after_stop_empties_the_contract() {
    let f = Fixture::new(true);
    f.at(STOP + 10_000);
    assert_eq!(f.client.withdraw(&None), DEPOSITED);
    assert_eq!(f.held(), 0);
    assert_eq!(f.client.remaining(), 0);
    f.assert_conserved();
}

#[test]
fn repeated_withdrawals_never_exceed_the_deposit() {
    let f = Fixture::new(true);
    for step in 1..=10u64 {
        f.at(START + step * 100);
        f.client.withdraw(&None);
        f.assert_conserved();
    }
    f.at(STOP + 1);
    assert_eq!(f.client.get().withdrawn, DEPOSITED);
    assert_eq!(f.held(), 0);
}

#[test]
#[should_panic]
fn withdraw_requires_the_recipients_authorization() {
    let te = TestEnv::at(START);
    let (token_client, sender) = te.make_token(MINT);
    let token = token_client.address.clone();
    let recipient = te.make_address();
    let env = te.env;
    let c = StreamContractClient::new(&env, &env.register(StreamContract, ()));
    c.create(&sender, &recipient, &token, &RATE, &START, &STOP, &true);

    // Drop the mocks, then withdraw with nobody authorized.
    env.set_auths(&[]);
    env.ledger().with_mut(|l| l.timestamp = START + 100);
    c.withdraw(&None);
}

// ------------------------------------------------------------------ cancel

#[test]
fn cancel_settles_the_recipient_and_refunds_the_sender() {
    let f = Fixture::new(true);
    let sender_before = f.token.balance(&f.sender);
    f.at(START + 400);
    f.client.cancel();

    assert_eq!(f.token.balance(&f.recipient), RATE * 400);
    assert_eq!(
        f.token.balance(&f.sender),
        sender_before + DEPOSITED - RATE * 400
    );
    assert_eq!(f.held(), 0);
    assert_eq!(f.client.remaining(), 0);
    f.assert_conserved();
}

#[test]
fn cancel_emits_the_cancelled_event() {
    let f = Fixture::new(true);
    f.at(START + 400);
    f.client.cancel();

    let events = f.env.events().all();
    // Created + Cancelled
    assert_eq!(events.len(), 2);
    let expected = Cancelled {
        sender: f.sender.clone(),
        settled_to_recipient: RATE * 400,
        refunded_to_sender: DEPOSITED - RATE * 400,
        cancelled_at: START + 400,
    };
    assert_eq!(&events[1], &expected);
}

#[test]
fn cancel_is_rejected_when_the_stream_is_not_cancellable() {
    let f = Fixture::new(false);
    f.at(START + 400);
    assert_eq!(f.client.try_cancel(), Err(Ok(Error::StreamNotCancellable)));
    assert_eq!(f.held(), DEPOSITED);
}

#[test]
fn cancel_rejects_a_second_call() {
    let f = Fixture::new(true);
    f.at(START + 400);
    f.client.cancel();
    assert_eq!(f.client.try_cancel(), Err(Ok(Error::StreamCancelled)));
}

#[test]
fn cancel_after_stop_refunds_nothing_and_settles_everything() {
    let f = Fixture::new(true);
    f.at(STOP + 500);
    f.client.cancel();
    assert_eq!(f.token.balance(&f.recipient), DEPOSITED);
    assert_eq!(f.client.get().refunded, 0);
    f.assert_conserved();
}

#[test]
fn cancel_before_start_refunds_the_whole_deposit() {
    let f = Fixture::new(true);
    let sender_before = f.token.balance(&f.sender);
    // Still at `start`; nothing has accrued.
    f.client.cancel();
    assert_eq!(f.token.balance(&f.recipient), 0);
    assert_eq!(f.token.balance(&f.sender), sender_before + DEPOSITED);
    f.assert_conserved();
}

#[test]
fn cancel_settles_only_what_was_not_already_withdrawn() {
    let f = Fixture::new(true);
    f.at(START + 200);
    f.client.withdraw(&None);
    f.at(START + 500);
    f.client.cancel();

    assert_eq!(f.token.balance(&f.recipient), RATE * 500);
    assert_eq!(f.client.get().withdrawn, RATE * 500);
    f.assert_conserved();
}

#[test]
fn nothing_accrues_after_cancellation() {
    let f = Fixture::new(true);
    f.at(START + 300);
    f.client.cancel();
    f.at(STOP + 10_000);
    assert_eq!(f.client.balance_of(&f.recipient), 0);
    assert_eq!(f.client.try_withdraw(&None), Err(Ok(Error::InvalidAmount)));
    f.assert_conserved();
}

// ----------------------------------------------------------------- top_up

#[test]
fn top_up_extends_the_stop_by_the_span_it_buys() {
    let f = Fixture::new(true);
    f.client.top_up(&(RATE * 500));
    let s = f.client.get();
    assert_eq!(s.stop, STOP + 500);
    assert_eq!(s.deposited, DEPOSITED + RATE * 500);
    assert_eq!(f.held(), DEPOSITED + RATE * 500);
    f.assert_conserved();
}

/// Pins the wire shape indexers decode (SPEC.md `indexed_events`). The
/// expected value is spelled out literally rather than built from
/// `events::ToppedUp`, so renaming a topic or a field fails here.
#[test]
fn top_up_emits_the_topped_up_event() {
    let f = Fixture::new(true);
    f.client.top_up(&(RATE * 500));

    let env = &f.env;
    let amount: Val = (RATE * 500).into_val(env);
    let deposited: Val = (DEPOSITED + RATE * 500).into_val(env);
    let new_stop: Val = (STOP + 500).into_val(env);
    // Map data: keys are the field names, serialized in sorted order.
    let data: Val = map![
        env,
        (Symbol::new(env, "amount"), amount),
        (Symbol::new(env, "deposited"), deposited),
        (Symbol::new(env, "new_stop"), new_stop),
    ]
    .into_val(env);
    assert_eq!(
        env.events().all().filter_by_contract(&f.client.address),
        vec![
            env,
            (
                f.client.address.clone(),
                vec![
                    env,
                    Symbol::new(env, "stream").into_val(env),
                    Symbol::new(env, "topped_up").into_val(env),
                    f.sender.into_val(env),
                ],
                data,
            ),
        ]
    );
}

#[test]
fn top_up_rejects_an_amount_that_is_not_a_whole_number_of_seconds() {
    let f = Fixture::new(true);
    assert_eq!(
        f.client.try_top_up(&(RATE + 1)),
        Err(Ok(Error::InvalidAmount))
    );
    assert_eq!(f.client.try_top_up(&1), Err(Ok(Error::InvalidAmount)));
}

#[test]
fn top_up_rejects_non_positive_amounts() {
    let f = Fixture::new(true);
    assert_eq!(f.client.try_top_up(&0), Err(Ok(Error::InvalidAmount)));
    assert_eq!(f.client.try_top_up(&(-RATE)), Err(Ok(Error::InvalidAmount)));
}

#[test]
fn top_up_rejects_a_span_that_overflows_the_stop_timestamp() {
    // rate = 1, duration = 1 keeps `deposited` tiny, so this isolates the
    // `amount / rate` -> u64 conversion: `2^64` seconds is beyond `u64::MAX`,
    // so extending `stop` by it could not even be represented. The contract
    // must reject rather than silently truncate the seconds (which would
    // break the funding invariant).
    let te = TestEnv::at(START);
    let (token_client, sender) = te.make_token(i128::MAX);
    let token = token_client.address.clone();
    let recipient = te.make_address();
    let env = te.env;
    let c = StreamContractClient::new(&env, &env.register(StreamContract, ()));
    c.create(&sender, &recipient, &token, &1, &START, &(START + 1), &true);

    assert_eq!(
        c.try_top_up(&(1_i128 << 64)),
        Err(Ok(Error::InvalidTimeRange))
    );

    // The failed call changed nothing.
    let s = c.get();
    assert_eq!(s.stop, START + 1);
    assert_eq!(s.deposited, 1);
}

#[test]
fn top_up_is_rejected_after_cancellation() {
    let f = Fixture::new(true);
    f.client.cancel();
    assert_eq!(
        f.client.try_top_up(&(RATE * 10)),
        Err(Ok(Error::StreamCancelled))
    );
}

// ----------------------------------------------------------------- extend

#[test]
fn extend_pulls_the_funds_the_longer_span_requires() {
    let f = Fixture::new(true);
    let sender_before = f.token.balance(&f.sender);
    f.client.extend(&(STOP + 300));

    let s = f.client.get();
    assert_eq!(s.stop, STOP + 300);
    assert_eq!(s.deposited, DEPOSITED + RATE * 300);
    assert_eq!(f.token.balance(&f.sender), sender_before - RATE * 300);
    f.assert_conserved();
}

#[test]
fn extend_rejects_a_stop_that_is_not_later() {
    let f = Fixture::new(true);
    for bad in [STOP, STOP - 1, 0] {
        assert_eq!(
            f.client.try_extend(&bad),
            Err(Ok(Error::StreamNotExtendable)),
            "new_stop {bad} was accepted"
        );
    }
}

#[test]
fn extend_rejects_funding_that_overflows() {
    // rate = i128::MAX over a 1-second span funds exactly i128::MAX (the
    // boundary pinned by `create_allows_the_exact_max_funding_boundary`).
    // Extending by 2 more seconds needs MAX * 2, which overflows i128, so
    // `extend` must report Overflow rather than wrap.
    let te = TestEnv::at(START);
    let (token_client, sender) = te.make_token(i128::MAX);
    let token = token_client.address.clone();
    let recipient = te.make_address();
    let c = StreamContractClient::new(&te.env, &te.env.register(StreamContract, ()));
    c.create(
        &sender,
        &recipient,
        &token,
        &i128::MAX,
        &START,
        &(START + 1),
        &true,
    );

    assert_eq!(c.try_extend(&(START + 3)), Err(Ok(Error::Overflow)));

    // The failed call changed nothing.
    let s = c.get();
    assert_eq!(s.stop, START + 1);
    assert_eq!(s.deposited, i128::MAX);
}

#[test]
fn extend_is_rejected_after_cancellation() {
    let f = Fixture::new(true);
    f.client.cancel();
    assert_eq!(
        f.client.try_extend(&(STOP + 100)),
        Err(Ok(Error::StreamCancelled))
    );
}

/// Pins the wire shape indexers decode (SPEC.md `indexed_events`). The
/// expected value is spelled out literally rather than built from
/// `events::Extended`, so renaming a topic or a field fails here.
#[test]
fn extend_emits_the_extended_event() {
    let f = Fixture::new(true);
    let new_stop_val = STOP + 300;
    f.client.extend(&new_stop_val);

    let env = &f.env;
    let added: Val = (RATE * 300).into_val(env);
    let deposited: Val = (DEPOSITED + RATE * 300).into_val(env);
    let new_stop: Val = new_stop_val.into_val(env);
    // Map data: keys are the field names, serialized in sorted order.
    let data: Val = map![
        env,
        (Symbol::new(env, "added"), added),
        (Symbol::new(env, "deposited"), deposited),
        (Symbol::new(env, "new_stop"), new_stop),
    ]
    .into_val(env);
    assert_eq!(
        env.events().all().filter_by_contract(&f.client.address),
        vec![
            env,
            (
                f.client.address.clone(),
                vec![
                    env,
                    Symbol::new(env, "stream").into_val(env),
                    Symbol::new(env, "extended").into_val(env),
                    f.sender.into_val(env),
                ],
                data,
            ),
        ]
    );
}

// -------------------------------------------------------------- balance_of

#[test]
fn balance_of_reports_each_partys_position() {
    let f = Fixture::new(true);
    f.at(START + 250);
    assert_eq!(f.client.balance_of(&f.recipient), RATE * 250);
    assert_eq!(f.client.balance_of(&f.sender), DEPOSITED - RATE * 250);
    assert_eq!(f.client.balance_of(&f.outsider), 0);
}

#[test]
fn balance_of_sender_is_zero_once_cancelled() {
    let f = Fixture::new(true);
    f.at(START + 250);
    f.client.cancel();
    assert_eq!(f.client.balance_of(&f.sender), 0);
    assert_eq!(f.client.balance_of(&f.recipient), 0);
}

// ---------------------------------------------------- conservation, mixed

#[test]
fn conservation_holds_across_a_full_mixed_lifecycle() {
    let f = Fixture::new(true);
    f.assert_conserved();

    f.at(START + 100);
    f.client.withdraw(&Some(RATE * 50));
    f.assert_conserved();

    f.client.top_up(&(RATE * 200));
    f.assert_conserved();

    f.at(START + 400);
    f.client.extend(&(STOP + 500));
    f.assert_conserved();

    f.client.withdraw(&None);
    f.assert_conserved();

    f.at(START + 900);
    f.client.cancel();
    f.assert_conserved();

    // Everything is out, and the two sides sum to what went in.
    let s = f.client.get();
    assert_eq!(f.held(), 0);
    assert_eq!(s.withdrawn + s.refunded, s.deposited);
    assert_eq!(
        f.token.balance(&f.recipient) + f.token.balance(&f.sender),
        MINT
    );
}

#[test]
fn deposited_calculation_matches_across_top_ups_and_extends() {
    let f = Fixture::new(true);
    let mut s = f.client.get();
    assert_eq!(s.deposited, RATE * (s.stop - s.start) as i128);

    // First top_up: add 100 seconds of funding
    f.client.top_up(&(RATE * 100));
    s = f.client.get();
    assert_eq!(s.deposited, RATE * (s.stop - s.start) as i128);

    // First extend: move stop forward by 50 seconds
    f.client.extend(&(s.stop + 50));
    s = f.client.get();
    assert_eq!(s.deposited, RATE * (s.stop - s.start) as i128);

    // Second top_up: add 200 seconds
    f.client.top_up(&(RATE * 200));
    s = f.client.get();
    assert_eq!(s.deposited, RATE * (s.stop - s.start) as i128);

    // Second extend: move stop forward by 75 seconds
    f.client.extend(&(s.stop + 75));
    s = f.client.get();
    assert_eq!(s.deposited, RATE * (s.stop - s.start) as i128);

    // Third top_up: add 150 seconds
    f.client.top_up(&(RATE * 150));
    s = f.client.get();
    assert_eq!(s.deposited, RATE * (s.stop - s.start) as i128);

    // Third extend: move stop forward by 300 seconds
    f.client.extend(&(s.stop + 300));
    s = f.client.get();
    assert_eq!(s.deposited, RATE * (s.stop - s.start) as i128);

    // Verify conservation still holds after all operations
    f.assert_conserved();
}

// ----------------------------------------------- negative auth tests (#109)

#[test]
#[should_panic]
fn cancel_requires_the_senders_authorization() {
    let f = Fixture::new(true);
    f.at(START + 100);
    // Clear auths so require_auth bites.
    f.env.set_auths(&[]);
    f.client.cancel();
}

#[test]
#[should_panic]
fn top_up_requires_the_senders_authorization() {
    let f = Fixture::new(true);
    f.at(START + 100);
    // Clear auths so require_auth bites.
    f.env.set_auths(&[]);
    f.client.top_up(&(RATE * 100));
}

#[test]
#[should_panic]
fn extend_requires_the_senders_authorization() {
    let f = Fixture::new(true);
    f.at(START + 100);
    let s = f.client.get();
    // Clear auths so require_auth bites.
    f.env.set_auths(&[]);
    f.client.extend(&(s.stop + 100));
}
