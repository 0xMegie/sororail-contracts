// Test fixtures do plain arithmetic on known-small constants; the checked-math
// rule is for contract code.
#![allow(clippy::arithmetic_side_effects)]

use soroban_sdk::{
    map,
    testutils::{
        storage::Instance as _, Address as _, Events as _, Ledger as _, MockAuth, MockAuthInvoke,
    },
    token::{StellarAssetClient, TokenClient},
    vec,
    xdr::{ScErrorCode, ScErrorType},
    Address, Env, Event, IntoVal, Symbol, Val,
};
use sororail_common::{storage::INSTANCE_BUMP, testutils::TestEnv, Error};

use crate::{
    contract::{StreamContract, StreamContractClient},
    events::{Cancelled, Created, ToppedUp, Withdrawn},
    types::Stream,
};

#[test]
fn bump_restores_the_instance_ttl_after_ledger_advance() {
    let f = Fixture::new(true);
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
        let env = te.env().clone();

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
 test/issues-296-299-lifecycle-properties
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

#[test]
fn accrual_is_exact_at_a_rate_and_elapsed_that_divide_badly() {
    // Unlike vesting's `total * elapsed / duration`, accrual never divides,
    // so there is no rounding direction to get wrong: it is always exactly
    // `rate * elapsed`, even for a rate/elapsed pair that would leave a
    // remainder under any proportional (divide-then-multiply) formulation.
    let env = Env::default();
    let mut s = bare(&env);
    s.rate_per_second = 7; // prime; divides unevenly into most spans
    s.stop = START + 3; // an odd, non-multiple-of-7 span
    s.deposited = 7 * 3;

    assert_eq!(s.accrued_at(START + 1), Ok(7));
    assert_eq!(s.accrued_at(START + 2), Ok(14));
    assert_eq!(s.accrued_at(START + 3), Ok(21));
    // Capped at `stop`, not truncated to some lower rounded figure.
    assert_eq!(s.accrued_at(START + 3), Ok(s.deposited));
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
    // `create` emits the token's own transfer event before its own
    // `Created`, so take the last event rather than assuming an index.
    let all = f.env.events().all();
    let events = all.events();
    let last = events.last().expect("no events emitted");
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
    assert_eq!(last, &expected.to_xdr(&f.env, &f.client.address));
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
    let env = te.env().clone();
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
    let env = te.env().clone();
    let client = StreamContractClient::new(&env, &env.register(StreamContract, ()));
    assert_eq!(
        client.try_create(
            &sender,
            &sender,
            &token.address,
            &RATE,
            &START,
            &STOP,
            &true
        ),
        Err(Ok(Error::IdenticalParties))
    );
    assert_eq!(token.balance(&sender), MINT);
    assert_eq!(token.balance(&client.address), 0);
    assert_eq!(client.try_get(), Err(Ok(Error::NotInitialized)));
}

/// `create` saves the stream before pulling funds. If the token transfer
/// fails, the host reverts the whole invocation 鈥� including that save 鈥� so a
/// later `get` still sees `NotInitialized`.
#[test]
fn create_reverts_cleanly_when_sender_cannot_fund() {
    let te = TestEnv::at(START);
    // Mint far less than the deposit the span requires.
    let (token_client, sender) = te.make_token(RATE);
    let token = token_client.address.clone();
    let recipient = te.make_address();
    let env = te.env().clone();
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
    let funding = RATE * i128::from(STOP - START);
    StellarAssetClient::new(&env, &token).mint(&sender, &funding);
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

    // `withdraw` emits the token's own transfer event before its own
    // `Withdrawn`, so take the last event rather than assuming an index.
    let all = f.env.events().all();
    let events = all.events();
    let last = events.last().expect("no events emitted");
    let expected = Withdrawn {
        recipient: f.recipient.clone(),
        amount: RATE * 300,
        total_withdrawn: RATE * 300,
    };
    assert_eq!(last, &expected.to_xdr(&f.env, &f.client.address));
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
    let env = te.env().clone();
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

    // `cancel` pays out up to two token transfers before its own
    // `Cancelled`, so take the last event rather than assuming an index.
    let all = f.env.events().all();
    let events = all.events();
    let last = events.last().expect("no events emitted");
    let expected = Cancelled {
        sender: f.sender.clone(),
        settled_to_recipient: RATE * 400,
        refunded_to_sender: DEPOSITED - RATE * 400,
        cancelled_at: START + 400,
    };
    assert_eq!(last, &expected.to_xdr(&f.env, &f.client.address));
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

/// Only the sender may cancel: a caller authorizing as anyone else hits the
/// host's auth check, not a contract error, and the stream is left intact.
#[test]
fn cancel_is_rejected_for_a_non_sender_caller() {
    let f = Fixture::new(true);
    f.at(START + 400);
    let held = f.held();
    let sender_balance = f.token.balance(&f.sender);
    let recipient_balance = f.token.balance(&f.recipient);

    for caller in [&f.outsider, &f.recipient] {
        f.env.mock_auths(&[MockAuth {
            address: caller,
            invoke: &MockAuthInvoke {
                contract: &f.client.address,
                fn_name: "cancel",
                args: ().into_val(&f.env),
                sub_invokes: &[],
            },
        }]);
        let res = f.env.try_invoke_contract::<(), soroban_sdk::Error>(
            &f.client.address,
            &Symbol::new(&f.env, "cancel"),
            vec![&f.env],
        );
        assert_eq!(
            res,
            Err(Ok(soroban_sdk::Error::from_type_and_code(
                ScErrorType::Auth,
                ScErrorCode::InvalidAction,
            )))
        );
    }

    assert_eq!(f.client.get().cancelled_at, None);
    assert_eq!(f.held(), held);
    assert_eq!(f.token.balance(&f.sender), sender_balance);
    assert_eq!(f.token.balance(&f.recipient), recipient_balance);
}

#[test]
fn second_cancel_errors_without_moving_tokens() {
    let f = Fixture::new(true);
    f.at(START + 400);
    f.client.cancel();
    let after_first = f.client.get();
    let held = f.held();
    let sender_balance = f.token.balance(&f.sender);
    let recipient_balance = f.token.balance(&f.recipient);

    f.at(START + 800);
    assert_eq!(f.client.try_cancel(), Err(Ok(Error::StreamCancelled)));

    assert_eq!(f.client.get(), after_first);
    assert_eq!(f.held(), held);
    assert_eq!(f.token.balance(&f.sender), sender_balance);
    assert_eq!(f.token.balance(&f.recipient), recipient_balance);
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

/// Across repeated top-ups the event carries only the amount just added,
/// while `deposited` and `new_stop` report the cumulative totals.
#[test]
fn top_up_event_carries_the_added_amount_and_the_new_total() {
    let f = Fixture::new(true);
    f.client.top_up(&(RATE * 500));
    f.client.top_up(&(RATE * 250));

    // `top_up` emits the token's transfer event before its own `ToppedUp`,
    // so take the last event rather than assuming an index.
    let all = f.env.events().all();
    let events = all.events();
    let last = events.last().expect("no events emitted");
    let expected = ToppedUp {
        sender: f.sender.clone(),
        amount: RATE * 250,
        new_stop: STOP + 750,
        deposited: DEPOSITED + RATE * 750,
    };
    assert_eq!(last, &expected.to_xdr(&f.env, &f.client.address));
    assert_eq!(f.client.get().deposited, DEPOSITED + RATE * 750);
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
    let env = te.env().clone();
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

#[test]
fn top_up_after_cancellation_moves_no_tokens_and_leaves_the_stream_unchanged() {
    let f = Fixture::new(true);
    f.at(START + 400);
    f.client.cancel();
    let before = f.client.get();
    let held = f.held();
    let sender_balance = f.token.balance(&f.sender);

    assert_eq!(
        f.client.try_top_up(&(RATE * 100)),
        Err(Ok(Error::StreamCancelled))
    );

    assert_eq!(f.client.get(), before);
    assert_eq!(f.held(), held);
    assert_eq!(f.token.balance(&f.sender), sender_balance);
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
    let c = StreamContractClient::new(te.env(), &te.env().register(StreamContract, ()));
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
    assert_eq!(s.deposited, RATE * i128::from(s.stop - s.start));

    // First top_up: add 100 seconds of funding
    f.client.top_up(&(RATE * 100));
    s = f.client.get();
    assert_eq!(s.deposited, RATE * i128::from(s.stop - s.start));

    // First extend: move stop forward by 50 seconds
    f.client.extend(&(s.stop + 50));
    s = f.client.get();
    assert_eq!(s.deposited, RATE * i128::from(s.stop - s.start));

    // Second top_up: add 200 seconds
    f.client.top_up(&(RATE * 200));
    s = f.client.get();
    assert_eq!(s.deposited, RATE * i128::from(s.stop - s.start));

    // Second extend: move stop forward by 75 seconds
    f.client.extend(&(s.stop + 75));
    s = f.client.get();
    assert_eq!(s.deposited, RATE * i128::from(s.stop - s.start));

    // Third top_up: add 150 seconds
    f.client.top_up(&(RATE * 150));
    s = f.client.get();
    assert_eq!(s.deposited, RATE * i128::from(s.stop - s.start));

    // Third extend: move stop forward by 300 seconds
    f.client.extend(&(s.stop + 300));
    s = f.client.get();
    assert_eq!(s.deposited, RATE * i128::from(s.stop - s.start));

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

// ---------------------------------------------------------------- property

mod accrued_at_properties {
    use proptest::prelude::*;

    use super::*;

    fn arb_stream(env: &Env) -> Stream {
        Stream {
            sender: Address::generate(env),
            recipient: Address::generate(env),
            token: Address::generate(env),
            rate_per_second: 0,
            start: 0,
            stop: 1,
            cancellable: false,
            deposited: 0,
            withdrawn: 0,
            refunded: 0,
            cancelled_at: None,
        }
    }

    proptest! {
        /// `accrued_at` can never exceed `deposited`, no matter how far past
        /// `stop` -- or how large a rate -- it is asked about. The stream is
        /// only ever funded for `rate_per_second * (stop - start)`, so
        /// accrual cannot pay out more than was ever put in.
        #[test]
        fn accrued_at_never_exceeds_deposit(
            rate_per_second in 0i128..=1_000_000_000,
            start in 0u64..=1_000_000_000,
            span in 1u64..=10_000_000,
            at_offset in 0u64..=20_000_000,
        ) {
            let env = Env::default();
            let stop = start + span;
            let deposited = rate_per_second * i128::from(span);

            let stream = Stream {
                rate_per_second,
                start,
                stop,
                deposited,
                ..arb_stream(&env)
            };

            let at = start.saturating_add(at_offset);
            let accrued = stream
                .accrued_at(at)
                .expect("accrued_at must not error for these bounded inputs");

            prop_assert!(
                accrued <= deposited,
                "accrued_at({at}) = {accrued} exceeds deposited = {deposited} (rate={rate_per_second}, start={start}, stop={stop})"
            );
            prop_assert!(accrued >= 0, "accrued_at({at}) = {accrued} is negative");
        }

        /// Cancellation freezes accrual at the cancellation timestamp, same
        /// as `stop` would -- and the bound against `deposited` still holds.
        #[test]
        fn accrued_at_respects_cancellation_and_the_deposit_bound(
            rate_per_second in 0i128..=1_000_000_000,
            start in 0u64..=1_000_000_000,
            span in 1u64..=10_000_000,
            cancel_offset in 0u64..=10_000_000,
            at_offset in 0u64..=20_000_000,
        ) {
            let env = Env::default();
            let stop = start + span;
            let deposited = rate_per_second * i128::from(span);
            let cancelled_at = start.saturating_add(cancel_offset).min(stop);

            let stream = Stream {
                rate_per_second,
                start,
                stop,
                deposited,
                cancelled_at: Some(cancelled_at),
                ..arb_stream(&env)
            };

            let at = start.saturating_add(at_offset);
            let accrued = stream
                .accrued_at(at)
                .expect("accrued_at must not error for these bounded inputs");

            prop_assert!(accrued <= deposited);

            let at_cancellation = stream
                .accrued_at(cancelled_at)
                .expect("accrued_at(cancelled_at) must not error");
            let long_after = stream
                .accrued_at(cancelled_at.saturating_add(at_offset))
                .expect("accrued_at(long after cancellation) must not error");
            prop_assert_eq!(at_cancellation, long_after);
        }
    }
}

fn accrual_is_linear_between_start_and]u髆GР陬澠瓂軄偷叡睍托佌攻衻箿赴佂焉浇撂(€€€€技紒櫧葋a14皝饺佈亯呁攣展バ亼暀ス晲亯鋪选攣呀瓡笣虂憰崶祬碧じ(€€€伭請伾呇晑習蓕蜁嵔箰鑱つ揉�(€€€€技紒1晳潟葋靴禃脱叺纴呅佪ˉ崰亝崓烧叞亯暆ス贪仴竵蜁嵔箲虂庭箥攣选攣U攻�(€€€€技紒暳綅牳(€€€伭請佂褏尚鑱载邪(€€€€技紒1晳潟葋靴禃脱叺纴呅佪ˉ崰亝崓烧叞仌箲贪仴竵蜁嵔箲虂庭箥攣选攣U攻�(€€€€技紒暳綅牳(€€€伭請佂呀黎佋匦�(€€€€技紒]选暼佈佂暪憰葋祬鋪崊箥暟亯暀缴攣佂呀羳�(€€€伭請亶吂崟北厜睌鑱壗桨�(€€€€技紒Q窖叞仌贂葋琳北晲仚山磥选攣蜁箲暼皝ジ佈佈江暩澨佂祬北曂衼展バ竵陛呭�(€€€€技紒伾呇晑習蓕蜁嵔箰€█⊥呀纮磥脱吷啸€�(€€€伭請亼暳酵パ晲鑱つ揉�(€€€€技紒Q窖叞仌贂葋羺伣招佈紒选攣蓵崶隶暪邪仴竵选攣呀瓡笣虂偷叡睍托佌攻懈(€€€伭請佪パ蓞莞鑱つ揉�(€€€€技紒Q窖叞仌贂葋蓵颜晒晲佈紒选攣蜁箲暼伣竵崊箥暠眳靴礁皝ジ佈佈江暩澨(€€€€技紒偷叡睍托佌攻懈(€€€伭請伾暀展憰愯仱娜喟(€€€€技紒]竵选攣脱蓵叴佪吿亶吂崟北晲皝仴衼輩贪仴竵蜁嵔箲虂庭箥攣选攣U攻�(€€€€技紒暳綅牳�崓烧叞佂呀撂仭暽敻(€€€伭請亶吂崟北晳}呅鑱=裂ソ格载续�)�()サ涟丮焉晠磥�(€€€€技紒Q窖叞亝崓烧晲佈紒选攣蓵崶隶暪衼吿伣榿亝褉皝荨曆葋荪选懮呡竵饺伖叫�(€€€€技�(€€€€技紒A丈旇伖紒脱缴厺敯伖紒暪佶山沟暪懈�
眳盗晲亝衼壗褷仌箲虁创伖窖ˉ箿(€€€€技紒厤嵣諘虂墪櫧蓴亖脱吷褉皝吂悂厤嵣諈皝毖虂呅亖脱搅€伣葋呅佈(€€€€技紒崊箥暠眳靴礁佈サ曂褏道皝荨贂葋嵔禃虂櫏赏懈丵伾曂毡衼崊竵箷贂�(€€€€技紒曖崟晲亖憰两庭褧憖�(€€€伭請仚竵厤嵣諘憓呅牂蜁睒皝呅鑱载肖€带両曂毡旭つ揉皝缮饺鴣�(€€€€€€€伇曅伒招仌櫃晬靴贁€魜呅�(€€€€€€€仴榿睍衼M降敗崊箥暠睍悿€魜蜁睒箥吂崟北晳}呅侅(€€€€€€€€€€€仌櫃晬靴贁€魜暀檿嵮ベ敼单浮崊箥暠睍悿�(€€€€€€€侓(€€€€€€€仌櫃晬靴贁€魜暀檿嵮ベ敼单浮蜁睒雇呀坤�((€€€€€€€仴榿暀檿嵮ベ攢痿佂暠樄脱吷衼�(€€€€€€€€€€€伾曆丈竵=瑺坤�(€€€€€€€侓(€€€€€€€伇曅仌眳镣晲€魜祬褷栝驼垺つ揉栝櫳酱櫃晬靴贁ぐ仱娜噼闄山础蜁睒雇褏尚い(€€€€€€€伇曅亝崓烧晲€魜祬褷栝嫡啊蜁睒股呇晑習蓕蜁嵔箰皝暠吜蜁悿(€€€€€€€�=祬褷栝单浮厤嵣諘惏佂暠樄憰两庭褧悿�(€€€侓((€€€€技紒崓烧晲亯招伖叫佸曅佪パ蓞莞皝吿伣榿亝褉竵]衼选攣蓵崶隶暪衼崊竵褏瓟�(€€€伭請仚竵呝叆眳壉晑呅牂蜁睒皝呅鑱载肖€带両曂毡旭つ揉皝缮饺鴣�(€€€€€€€伇曅亝崓烧晲€魜蜁睒箙崓烧晳}呅肖(€€€€€€€仴榿蜁睒馆パ蓞莞€鴣厤嵣諘悂�(€€€€€€€€€€€伾曆丈竵扇�缮饺栝U箲暽櫛杰れ(€€€€€€€侓(€€€€€€€伒呇犺橥請崓烧晲皝蜁睒馆パ蓞莞�(€€€侓((€€€€技紒]衼选攣嵔寡蓞嵭佂靴卑仭奖懱仚饺佈ˉ虂脱蓵叴�(€€€伭請仚竵蓵祬スス湢櫷暠槫€带両曂毡旭つ揉皝缮饺鴣�(€€€€€€€伒呇犺橥請〉呇犺橥請⊥暠樄憰两庭褧惏佂暠樄荪选懮呡袱佂暠樄蓵櫿箲晲�(€€€侓((€€€€技紒]选暼佈佂焉晠磥虂墪暩亶吂崟北晲�(€€€€技�(€€€€技紒Q伇晬鍗睌仴虂墺箙射亝箰伣箶递呬皝图仴衼ヌ伒綉暠睍悂変佈(€€€€技紒亶吂崟北晳}呇€佈サ曂褏道€≥ˉ崰亝崓烧叞伖晻懱亝瑰輩浃伾呇葋选吀亜(€€€€技紒丮褏褧€仌拐磥饱瓟仌蛵山軡谈丮晹€�1晬鍗睌伒綉暠ス湀仴竵MA沟惛(€€€伭請仚竵ネ}崊箥暠睍悹櫷暠槫€带亯浇皝�(€€€€€€€佂暠樄崊箥暠睍憓呅攻蛚徒禂牑(€€€侓((€€€€技紒Q亼晬眳蓵悂土吀仴竵蜁嵔箲谈(€€€伭請仚竵懻蓞靴礁牂蜁睒带佋匦侅(€€€€€€€佂暠樄脱嚼雇呇丈呇ス潁驼垺蜁睒雇褏尚�(€€€侓)�(
 main
