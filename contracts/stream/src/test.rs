use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token::{StellarAssetClient, TokenClient},
    Address, Env,
};
use sororail_common::Error;

use crate::{
    contract::{StreamContract, StreamContractClient},
    types::Stream,
};

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
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = START);

        let sender = Address::generate(&env);
        let recipient = Address::generate(&env);
        let outsider = Address::generate(&env);

        let issuer = Address::generate(&env);
        let token_address = env.register_stellar_asset_contract_v2(issuer).address();
        StellarAssetClient::new(&env, &token_address).mint(&sender, &MINT);

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
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = START);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    let issuer = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(issuer).address();
    StellarAssetClient::new(&env, &token).mint(&sender, &MINT);
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
fn extend_is_rejected_after_cancellation() {
    let f = Fixture::new(true);
    f.client.cancel();
    assert_eq!(
        f.client.try_extend(&(STOP + 100)),
        Err(Ok(Error::StreamCancelled))
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
