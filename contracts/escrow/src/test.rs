use soroban_sdk::{
    testutils::{storage::Instance as _, Address as _, Events as _, Ledger as _},
 test/consolidate-contract-helpers-and-stable-ids
    token::TokenClient,
    Address, Env,

    token::{StellarAssetClient, TokenClient},
    Address, Env, Event,
 main
};
use sororail_common::{storage::INSTANCE_BUMP, testutils::TestEnv, Error};

use crate::{
    contract::{EscrowContract, EscrowContractClient},
    events::{Cancelled, Created, Disputed, Funded, Refunded, Released, Resolved},
    types::State,
};

#[test]
fn bump_restores_the_instance_ttl_after_ledger_advance() {
    let f = Fixture::new(false);
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

const AMOUNT: i128 = 1_000_000;
const START_TS: u64 = 1_000_000;
const DEADLINE: u64 = START_TS + 10_000;

struct Fixture<'a> {
    env: Env,
    client: EscrowContractClient<'a>,
    token: TokenClient<'a>,
    depositor: Address,
    beneficiary: Address,
    arbiter: Address,
    outsider: Address,
}

impl Fixture<'_> {
    /// Builds an initialized escrow. `with_arbiter` controls whether the
    /// dispute path is available.
    fn new(with_arbiter: bool) -> Self {
        let te = TestEnv::at(START_TS);
        let (token, depositor) = te.make_token(AMOUNT * 10);
        let token_address = token.address.clone();
        let beneficiary = te.make_address();
        let arbiter = te.make_address();
        let outsider = te.make_address();
        let env = te.env().clone();

        let contract_id = env.register(EscrowContract, ());
        let client = EscrowContractClient::new(&env, &contract_id);

        client.init(
            &depositor,
            &beneficiary,
            &if with_arbiter {
                Some(arbiter.clone())
            } else {
                None
            },
            &token_address,
            &AMOUNT,
            &DEADLINE,
        );

        Fixture {
            token: TokenClient::new(&env, &token_address),
            env,
            client,
            depositor,
            beneficiary,
            arbiter,
            outsider,
        }
    }

    fn funded(with_arbiter: bool) -> Self {
        let f = Fixture::new(with_arbiter);
        f.client.fund();
        f
    }

    fn escrow_balance(&self) -> i128 {
        self.token.balance(&self.client.address)
    }

    fn advance_past_deadline(&self) {
        self.env.ledger().with_mut(|l| l.timestamp = DEADLINE + 1);
    }
}

// ------------------------------------------------------------------ init

#[test]
fn init_stores_the_agreement() {
    let f = Fixture::new(true);
    let e = f.client.get();
    assert_eq!(e.depositor, f.depositor);
    assert_eq!(e.beneficiary, f.beneficiary);
    assert_eq!(e.arbiter, Some(f.arbiter.clone()));
    assert_eq!(e.amount, AMOUNT);
    assert_eq!(e.deadline, DEADLINE);
    assert_eq!(e.state, State::Created);
    // Nothing has moved yet.
    assert_eq!(f.escrow_balance(), 0);
}

#[test]
fn init_rejects_a_second_call() {
    let f = Fixture::new(true);
    let res = f.client.try_init(
        &f.depositor,
        &f.beneficiary,
        &None,
        &f.token.address,
        &AMOUNT,
        &DEADLINE,
    );
    assert_eq!(res, Err(Ok(Error::AlreadyInitialized)));
}

#[test]
fn init_rejects_non_positive_amounts() {
    let env = TestEnv::at(START_TS).env;
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = START_TS);
    let client = EscrowContractClient::new(&env, &env.register(EscrowContract, ()));
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let t = Address::generate(&env);

    for bad in [0i128, -1, i128::MIN] {
        let res = client.try_init(&a, &b, &None, &t, &bad, &DEADLINE);
        assert_eq!(
            res,
            Err(Ok(Error::InvalidAmount)),
            "amount {bad} was accepted"
        );
    }
}

#[test]
fn init_rejects_a_deadline_that_is_not_in_the_future() {
    let env = TestEnv::at(START_TS).env;
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = START_TS);
    let client = EscrowContractClient::new(&env, &env.register(EscrowContract, ()));
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let t = Address::generate(&env);

    // Equal to now, and in the past.
    for bad in [START_TS, START_TS - 1, 0] {
        let res = client.try_init(&a, &b, &None, &t, &AMOUNT, &bad);
        assert_eq!(
            res,
            Err(Ok(Error::InvalidTimeRange)),
            "deadline {bad} was accepted"
        );
    }
}

#[test]
fn entry_points_error_before_init() {
    let env = TestEnv::new().env;
    env.mock_all_auths();
    let client = EscrowContractClient::new(&env, &env.register(EscrowContract, ()));
    let who = Address::generate(&env);

    assert_eq!(client.try_fund(), Err(Ok(Error::NotInitialized)));
    assert_eq!(client.try_cancel(), Err(Ok(Error::NotInitialized)));
    assert_eq!(client.try_release(&who), Err(Ok(Error::NotInitialized)));
    assert_eq!(client.try_refund(&who), Err(Ok(Error::NotInitialized)));
    assert_eq!(client.try_dispute(&who), Err(Ok(Error::NotInitialized)));
    assert_eq!(client.try_resolve(&5_000), Err(Ok(Error::NotInitialized)));
    assert_eq!(client.try_state(), Err(Ok(Error::NotInitialized)));
}

#[test]
#[should_panic]
fn init_requires_the_depositors_authorization() {
    // No mock_all_auths: require_auth must actually bite.
    let env = Env::default();
    env.ledger().with_mut(|l| l.timestamp = START_TS);
    let client = EscrowContractClient::new(&env, &env.register(EscrowContract, ()));
    client.init(
        &Address::generate(&env),
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &AMOUNT,
        &DEADLINE,
    );
}

#[test]
fn init_emits_created_event_with_correct_topics_and_data() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = START_TS);

    let depositor = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let issuer = Address::generate(&env);
    let token_address = env.register_stellar_asset_contract_v2(issuer).address();
    let client = EscrowContractClient::new(&env, &env.register(EscrowContract, ()));

    client.init(
        &depositor,
        &beneficiary,
        &Some(arbiter.clone()),
        &token_address,
        &AMOUNT,
        &DEADLINE,
    );

    let all = env.events().all();
    let events = all.events();
    assert_eq!(events.len(), 1);

    let created_event = &events[0];
    let expected = Created {
        depositor: depositor.clone(),
        beneficiary: beneficiary.clone(),
        token: token_address.clone(),
        amount: AMOUNT,
        deadline: DEADLINE,
    };
    assert_eq!(created_event, &expected.to_xdr(&env, &client.address));
}

// --------------------------------------------------------------- cancel

#[test]
fn cancel_transitions_created_to_refunded() {
    let f = Fixture::new(true);
    f.client.cancel();
    assert_eq!(f.client.state(), State::Refunded);
}

// `cancel` takes no caller argument -- it authorizes internally via
// `escrow.depositor.require_auth()` -- so "wrong caller" isn't expressible
// as a parameter under this fixture's blanket `mock_all_auths()`; that would
// need per-call `set_auths` to simulate a missing depositor signature, which
// is out of scope here. A prior version of this test called
// `try_cancel(&some_address)` against a signature `cancel()` no longer has.

#[test]
fn cancel_rejects_a_funded_escrow() {
    let f = Fixture::funded(true);
    assert_eq!(f.client.try_cancel(), Err(Ok(Error::EscrowNotCancellable)));
}

#[test]
fn cancel_emits_cancelled_event() {
    let f = Fixture::new(true);
    f.client.cancel();

    // `events().all()` only returns events from the *last* invocation, so
    // only `cancel`'s own `Cancelled` is visible here -- `init`'s `Created`
    // (from an earlier, separate call) already scrolled out of view.
    let all = f.env.events().all();
    let events = all.events();
    assert_eq!(events.len(), 1);
    let expected = Cancelled {
        depositor: f.depositor.clone(),
    };
    assert_eq!(&events[0], &expected.to_xdr(&f.env, &f.client.address));
}

#[test]
fn cancel_does_not_move_any_tokens() {
    let f = Fixture::new(true);
    let depositor_before = f.token.balance(&f.depositor);
    f.client.cancel();

    assert_eq!(f.token.balance(&f.depositor), depositor_before);
    assert_eq!(f.escrow_balance(), 0);
}

// ------------------------------------------------------------------ fund

#[test]
fn fund_moves_the_tokens_into_the_contract() {
    let f = Fixture::new(true);
    let before = f.token.balance(&f.depositor);
    f.client.fund();

    assert_eq!(f.escrow_balance(), AMOUNT);
    assert_eq!(f.token.balance(&f.depositor), before - AMOUNT);
    assert_eq!(f.client.state(), State::Funded);
}

#[test]
fn fund_rejects_a_second_call() {
    let f = Fixture::funded(true);
    assert_eq!(f.client.try_fund(), Err(Ok(Error::EscrowNotFundable)));
}

/// `fund` writes `State::Funded` before the token pull. If the depositor cannot
/// pay, the host reverts the invocation so a later `state()` is still `Created`.
#[test]
fn fund_reverts_cleanly_when_depositor_cannot_pay() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = START_TS);

    let depositor = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let issuer = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(issuer);
    let token_address = sac.address();
    // Deliberately mint nothing to the depositor.
    let token = TokenClient::new(&env, &token_address);

    let client = EscrowContractClient::new(&env, &env.register(EscrowContract, ()));
    client.init(
        &depositor,
        &beneficiary,
        &Some(arbiter),
        &token_address,
        &AMOUNT,
        &DEADLINE,
    );
    assert_eq!(client.state(), State::Created);

    assert!(client.try_fund().is_err());
    assert_eq!(
        client.state(),
        State::Created,
        "optimistic Funded write leaked"
    );
    assert_eq!(token.balance(&client.address), 0);
    assert_eq!(client.get().amount, AMOUNT);
}

#[test]
fn fund_rejects_a_closed_escrow() {
    let f = Fixture::funded(true);
    f.client.release(&f.depositor);
    assert_eq!(f.client.try_fund(), Err(Ok(Error::EscrowClosed)));
}

/// `fund` must reject when the deadline has already passed, even if the
/// escrow is still in `Created` state (#102).
#[test]
fn fund_rejects_after_the_deadline() {
    let f = Fixture::new(true);
    f.advance_past_deadline();
    assert_eq!(f.client.try_fund(), Err(Ok(Error::DeadlinePassed)));
    assert_eq!(f.escrow_balance(), 0);
    assert_eq!(f.client.state(), State::Created);
}

/// `fund` must reject at exactly the deadline timestamp (#102).
///
/// Boundary: matches `refund_at_exactly_the_deadline_is_allowed` -- the
/// instant the depositor's unilateral refund right begins is also the
/// instant funding stops being allowed.
#[test]
fn fund_rejects_at_exactly_the_deadline() {
    let f = Fixture::new(true);
    f.env.ledger().with_mut(|l| l.timestamp = DEADLINE);
    assert_eq!(f.client.try_fund(), Err(Ok(Error::DeadlinePassed)));
    assert_eq!(f.client.state(), State::Created);
}

#[test]
fn fund_just_before_the_deadline_succeeds() {
    let f = Fixture::new(true);
    f.env.ledger().with_mut(|l| l.timestamp = DEADLINE - 1);
    f.client.fund();
    assert_eq!(f.client.state(), State::Funded);
}

#[test]
fn fund_emits_funded_event_with_correct_topics_and_data() {
    let f = Fixture::new(true);
    f.client.fund();

    let all = f.env.events().all();
    let events = all.events();
    assert_eq!(events.len(), 2); // Created + Funded

    let funded_event = &events[1];
    let expected = Funded {
        depositor: f.depositor.clone(),
        amount: AMOUNT,
    };
    assert_eq!(funded_event, &expected.to_xdr(&f.env, &f.client.address));
}

// --------------------------------------------------------------- release

#[test]
fn release_pays_the_beneficiary() {
    let f = Fixture::funded(true);
    f.client.release(&f.depositor);

    assert_eq!(f.token.balance(&f.beneficiary), AMOUNT);
    assert_eq!(f.escrow_balance(), 0);
    assert_eq!(f.client.state(), State::Released);
}

#[test]
fn release_emits_released_event_with_correct_topics_and_data() {
    let f = Fixture::funded(true);
    f.client.release(&f.depositor);

    // `release` emits the token's own transfer event before its own
    // `Released` (payment happens, then the event is published), so take
    // the last event rather than assuming an exact count or index.
    let all = f.env.events().all();
    let events = all.events();
    let last = events.last().expect("no events emitted");
    let expected = Released {
        beneficiary: f.beneficiary.clone(),
        amount: AMOUNT,
        released_by: f.depositor.clone(),
    };
    assert_eq!(last, &expected.to_xdr(&f.env, &f.client.address));
}

#[test]
fn release_is_callable_by_the_arbiter() {
    let f = Fixture::funded(true);
    f.client.release(&f.arbiter);
    assert_eq!(f.token.balance(&f.beneficiary), AMOUNT);
}

#[test]
fn release_is_not_callable_by_the_beneficiary() {
    // The point of the escrow: the payee cannot pay themselves.
    let f = Fixture::funded(true);
    assert_eq!(
        f.client.try_release(&f.beneficiary),
        Err(Ok(Error::Unauthorized))
    );
}

#[test]
fn release_is_not_callable_by_an_outsider() {
    let f = Fixture::funded(true);
    assert_eq!(
        f.client.try_release(&f.outsider),
        Err(Ok(Error::Unauthorized))
    );
}

#[test]
fn release_requires_funding_first() {
    let f = Fixture::new(true);
    assert_eq!(
        f.client.try_release(&f.depositor),
        Err(Ok(Error::EscrowNotFunded))
    );
}

#[test]
fn release_rejects_a_closed_escrow() {
    let f = Fixture::funded(true);
    f.client.release(&f.depositor);
    assert_eq!(
        f.client.try_release(&f.depositor),
        Err(Ok(Error::EscrowClosed))
    );
}

// ---------------------------------------------------------------- refund

#[test]
fn refund_by_depositor_is_blocked_before_the_deadline() {
    let f = Fixture::funded(true);
    assert_eq!(
        f.client.try_refund(&f.depositor),
        Err(Ok(Error::DeadlineNotReached))
    );
    // Funds stay put.
    assert_eq!(f.escrow_balance(), AMOUNT);
}

#[test]
fn refund_by_depositor_succeeds_after_the_deadline() {
    let f = Fixture::funded(true);
    let before = f.token.balance(&f.depositor);
    f.advance_past_deadline();
    f.client.refund(&f.depositor);

    assert_eq!(f.token.balance(&f.depositor), before + AMOUNT);
    assert_eq!(f.escrow_balance(), 0);
    assert_eq!(f.client.state(), State::Refunded);
}

#[test]
fn refund_emits_refunded_event_with_correct_topics_and_data() {
    let f = Fixture::funded(true);
    f.advance_past_deadline();
    f.client.refund(&f.depositor);

    // `refund` emits the token's own transfer event before its own
    // `Refunded`, so take the last event rather than assuming an index.
    let all = f.env.events().all();
    let events = all.events();
    let last = events.last().expect("no events emitted");
    let expected = Refunded {
        depositor: f.depositor.clone(),
        amount: AMOUNT,
        refunded_by: f.depositor.clone(),
    };
    assert_eq!(last, &expected.to_xdr(&f.env, &f.client.address));
}

#[test]
fn refund_by_arbiter_is_allowed_before_the_deadline() {
    let f = Fixture::funded(true);
    f.client.refund(&f.arbiter);
    assert_eq!(f.escrow_balance(), 0);
    assert_eq!(f.client.state(), State::Refunded);
}

#[test]
fn refund_is_not_callable_by_the_beneficiary_or_an_outsider() {
    let f = Fixture::funded(true);
    f.advance_past_deadline();
    assert_eq!(
        f.client.try_refund(&f.beneficiary),
        Err(Ok(Error::Unauthorized))
    );
    assert_eq!(
        f.client.try_refund(&f.outsider),
        Err(Ok(Error::Unauthorized))
    );
}

#[test]
fn refund_at_exactly_the_deadline_is_allowed() {
    // Boundary: the deadline is inclusive of the refund right.
    let f = Fixture::funded(true);
    f.env.ledger().with_mut(|l| l.timestamp = DEADLINE);
    f.client.refund(&f.depositor);
    assert_eq!(f.client.state(), State::Refunded);
}

// --------------------------------------------------------------- dispute

#[test]
fn dispute_requires_an_arbiter() {
    let f = Fixture::funded(false);
    assert_eq!(
        f.client.try_dispute(&f.depositor),
        Err(Ok(Error::EscrowNoArbiter))
    );
}

#[test]
fn dispute_is_callable_by_either_party() {
    for caller_is_depositor in [true, false] {
        let f = Fixture::funded(true);
        let caller = if caller_is_depositor {
            f.depositor.clone()
        } else {
            f.beneficiary.clone()
        };
        f.client.dispute(&caller);
        assert_eq!(f.client.state(), State::Disputed);
    }
}

#[test]
fn dispute_is_not_callable_by_the_arbiter_or_an_outsider() {
    let f = Fixture::funded(true);
    assert_eq!(
        f.client.try_dispute(&f.arbiter),
        Err(Ok(Error::Unauthorized))
    );
    assert_eq!(
        f.client.try_dispute(&f.outsider),
        Err(Ok(Error::Unauthorized))
    );
}

#[test]
fn dispute_rejects_a_second_call() {
    let f = Fixture::funded(true);
    f.client.dispute(&f.depositor);
    assert_eq!(
        f.client.try_dispute(&f.depositor),
        Err(Ok(Error::EscrowAlreadyDisputed))
    );
}

#[test]
fn dispute_requires_funding_first() {
    let f = Fixture::new(true);
    assert_eq!(
        f.client.try_dispute(&f.depositor),
        Err(Ok(Error::EscrowNotFunded))
    );
}

#[test]
fn dispute_emits_disputed_event_with_correct_topics_and_data() {
    let f = Fixture::funded(true);
    f.client.dispute(&f.depositor);

    // `events().all()` only returns events from the *last* invocation.
    let all = f.env.events().all();
    let events = all.events();
    assert_eq!(events.len(), 1);

    let disputed_event = &events[0];
    let expected = Disputed {
        raised_by: f.depositor.clone(),
    };
    assert_eq!(disputed_event, &expected.to_xdr(&f.env, &f.client.address));
}

#[test]
fn release_and_refund_are_blocked_while_disputed() {
    let f = Fixture::funded(true);
    f.client.dispute(&f.depositor);
    assert_eq!(
        f.client.try_release(&f.depositor),
        Err(Ok(Error::EscrowNotFunded))
    );
    f.advance_past_deadline();
    // Disputed-but-within-the-grace-period is special-cased ahead of the
    // ordinary state check, so this is `DeadlineNotReached` (the dispute
    // grace period, not the original deadline), not `EscrowNotFunded`.
    assert_eq!(
        f.client.try_refund(&f.depositor),
        Err(Ok(Error::DeadlineNotReached))
    );
    assert_eq!(f.escrow_balance(), AMOUNT);
}

// --------------------------------------------------------------- resolve

#[test]
fn resolve_splits_between_the_parties() {
    let f = Fixture::funded(true);
    f.client.dispute(&f.beneficiary);
    f.client.resolve(&2_500);

    assert_eq!(f.token.balance(&f.beneficiary), AMOUNT / 4);
    assert_eq!(f.escrow_balance(), 0);
    assert_eq!(f.client.state(), State::Resolved);
}

#[test]
fn resolve_requires_a_dispute() {
    let f = Fixture::funded(true);
    assert_eq!(
        f.client.try_resolve(&5_000),
        Err(Ok(Error::EscrowNotDisputed))
    );
}

#[test]
#[should_panic(expected = "dispute requires an arbiter")]
fn resolve_guards_against_a_missing_arbiter_even_if_disputed_is_reached() {
    // `dispute` already refuses to run without an arbiter
    // (`dispute_requires_an_arbiter`), so this state is unreachable through
    // the public API. `resolve`'s `.expect(...)` re-checks anyway as
    // defense-in-depth (#105) -- this proves that check actually fires,
    // rather than trusting it's dead code that happens to compile.
    let f = Fixture::funded(false);
    f.env.as_contract(&f.client.address, || {
        let mut escrow = crate::storage::load(&f.env).unwrap();
        escrow.state = State::Disputed;
        crate::storage::save(&f.env, &escrow);
    });

    f.client.resolve(&5_000);
}

#[test]
fn resolve_rejects_basis_points_above_the_maximum() {
    let f = Fixture::funded(true);
    f.client.dispute(&f.depositor);
    assert_eq!(
        f.client.try_resolve(&10_001),
        Err(Ok(Error::InvalidBasisPoints))
    );
    // Still resolvable afterwards -- the failed call changed nothing.
    assert_eq!(f.client.state(), State::Disputed);
}

#[test]
fn resolve_endpoints_pay_one_side_in_full() {
    let f = Fixture::funded(true);
    f.client.dispute(&f.depositor);
    f.client.resolve(&10_000);
    assert_eq!(f.token.balance(&f.beneficiary), AMOUNT);

    let g = Fixture::funded(true);
    let depositor_before = g.token.balance(&g.depositor);
    g.client.dispute(&g.depositor);
    g.client.resolve(&0);
    assert_eq!(g.token.balance(&g.depositor), depositor_before + AMOUNT);
    assert_eq!(g.token.balance(&g.beneficiary), 0);
}

#[test]
fn resolve_conserves_the_escrowed_amount_at_every_split() {
    // The invariant: paid_to_beneficiary + paid_to_depositor == amount,
    // exactly, including at splits that divide badly.
    for bps in [0u32, 1, 3, 333, 1_234, 5_000, 6_667, 9_999, 10_000] {
        let f = Fixture::funded(true);
        let depositor_before = f.token.balance(&f.depositor);
        f.client.dispute(&f.depositor);
        f.client.resolve(&bps);

        let to_beneficiary = f.token.balance(&f.beneficiary);
        let to_depositor = f.token.balance(&f.depositor) - depositor_before;

        assert_eq!(
            to_beneficiary + to_depositor,
            AMOUNT,
            "{bps}bps split leaked value"
        );
        assert_eq!(f.escrow_balance(), 0, "{bps}bps split stranded funds");
        assert!(to_beneficiary >= 0 && to_depositor >= 0);
    }
}

#[test]
fn init_with_beneficiary_as_contract_address_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = START_TS);

    let depositor = Address::generate(&env);
    let issuer = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(issuer);
    let token_address = sac.address();
    StellarAssetClient::new(&env, &token_address).mint(&depositor, &(AMOUNT * 10));

    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    // Using contract address as beneficiary should succeed (no-op transfer is harmless)
    let result = client.try_init(
        &depositor,
        &contract_id,
        &None,
        &token_address,
        &AMOUNT,
        &DEADLINE,
    );
    assert!(
        result.is_ok(),
        "init with contract as beneficiary should succeed"
    );
}

#[test]
fn init_with_depositor_as_contract_address_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = START_TS);

    let depositor = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let issuer = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(issuer);
    let token_address = sac.address();
    StellarAssetClient::new(&env, &token_address).mint(&depositor, &(AMOUNT * 10));

    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    // Using contract address as depositor should succeed
    let result = client.try_init(
        &contract_id,
        &beneficiary,
        &None,
        &token_address,
        &AMOUNT,
        &DEADLINE,
    );
    assert!(
        result.is_ok(),
        "init with contract as depositor should succeed"
    );
}

#[test]
fn init_rejects_multiple_invalid_args_in_order() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = START_TS);

    let depositor = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let issuer = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(issuer);
    let token_address = sac.address();
    StellarAssetClient::new(&env, &token_address).mint(&depositor, &(AMOUNT * 10));

    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    // Both AlreadyInitialized AND InvalidAmount: AlreadyInitialized comes first
    client.init(
        &depositor,
        &beneficiary,
        &None,
        &token_address,
        &AMOUNT,
        &DEADLINE,
    );
    assert_eq!(
        client.try_init(
            &depositor,
            &beneficiary,
            &None,
            &token_address,
            &0,
            &DEADLINE
        ),
        Err(Ok(Error::AlreadyInitialized))
    );

    // InvalidAmount AND InvalidTimeRange: InvalidAmount comes first
    let contract_id2 = env.register(EscrowContract, ());
    let client2 = EscrowContractClient::new(&env, &contract_id2);
    assert_eq!(
        client2.try_init(
            &depositor,
            &beneficiary,
            &None,
            &token_address,
            &0,
            &(START_TS - 1)
        ),
        Err(Ok(Error::InvalidAmount))
    );
}

#[test]
fn resolve_rejects_a_second_call() {
    let f = Fixture::funded(true);
    f.client.dispute(&f.depositor);
    f.client.resolve(&5_000);
    assert_eq!(
        f.client.try_resolve(&5_000),
        Err(Ok(Error::EscrowNotDisputed))
    );
}

#[test]
fn resolve_emits_resolved_event_with_correct_topics_and_data() {
    let f = Fixture::funded(true);
    f.client.dispute(&f.depositor);
    f.client.resolve(&2_500);

    // `resolve` emits up to two token transfer events before its own
    // `Resolved`, so take the last event rather than assuming an index.
    let all = f.env.events().all();
    let events = all.events();
    let resolved_event = events.last().expect("no events emitted");
    let expected = Resolved {
        arbiter: f.arbiter.clone(),
        split_bps: 2_500,
        to_beneficiary: AMOUNT / 4,
        to_depositor: AMOUNT * 3 / 4,
    };
    assert_eq!(resolved_event, &expected.to_xdr(&f.env, &f.client.address));
}

// -------------------------------------------------- negative auth tests (#108)

#[test]
#[should_panic]
fn fund_requires_the_depositors_authorization() {
    let f = Fixture::new(true);
    // Clear auths so require_auth bites.
    f.env.set_auths(&[]);
    f.client.fund();
}

#[test]
#[should_panic]
fn resolve_requires_the_arbiters_authorization() {
    let f = Fixture::funded(true);
    f.client.dispute(&f.depositor);
    // Clear auths so require_auth bites.
    f.env.set_auths(&[]);
    f.client.resolve(&5_000);
}

#[test]
#[should_panic]
fn release_requires_callers_authorization() {
    let f = Fixture::funded(true);
    // Clear auths so require_auth bites.
    f.env.set_auths(&[]);
    f.client.release(&f.depositor);
}
