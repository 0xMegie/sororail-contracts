use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token::{StellarAssetClient, TokenClient},
    Address, Env,
};
use sororail_common::Error;

use crate::{
    contract::{EscrowContract, EscrowContractClient},
    types::State,
};

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
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = START_TS);

        let depositor = Address::generate(&env);
        let beneficiary = Address::generate(&env);
        let arbiter = Address::generate(&env);
        let outsider = Address::generate(&env);

        let issuer = Address::generate(&env);
        let sac = env.register_stellar_asset_contract_v2(issuer);
        let token_address = sac.address();
        StellarAssetClient::new(&env, &token_address).mint(&depositor, &(AMOUNT * 10));

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
    let env = Env::default();
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
    let env = Env::default();
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
    let env = Env::default();
    env.mock_all_auths();
    let client = EscrowContractClient::new(&env, &env.register(EscrowContract, ()));
    let who = Address::generate(&env);

    assert_eq!(client.try_fund(), Err(Ok(Error::NotInitialized)));
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

#[test]
fn fund_rejects_a_closed_escrow() {
    let f = Fixture::funded(true);
    f.client.release(&f.depositor);
    assert_eq!(f.client.try_fund(), Err(Ok(Error::EscrowClosed)));
}

#[test]
fn fund_rejects_after_the_deadline_with_a_dedicated_error() {
    // Not `EscrowClosed`: the escrow is still `Created`, never funded, so
    // "closed" would misdescribe why the call failed.
    let f = Fixture::new(true);
    f.advance_past_deadline();
    assert_eq!(f.client.try_fund(), Err(Ok(Error::DeadlinePassed)));
    assert_eq!(f.escrow_balance(), 0);
    assert_eq!(f.client.state(), State::Created);
}

#[test]
fn fund_at_exactly_the_deadline_is_rejected() {
    // Boundary: matches `refund_at_exactly_the_deadline_is_allowed` -- the
    // instant the depositor's unilateral refund right begins is also the
    // instant funding stops being allowed.
    let f = Fixture::new(true);
    f.env.ledger().with_mut(|l| l.timestamp = DEADLINE);
    assert_eq!(f.client.try_fund(), Err(Ok(Error::DeadlinePassed)));
}

#[test]
fn fund_just_before_the_deadline_succeeds() {
    let f = Fixture::new(true);
    f.env.ledger().with_mut(|l| l.timestamp = DEADLINE - 1);
    f.client.fund();
    assert_eq!(f.client.state(), State::Funded);
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
fn release_and_refund_are_blocked_while_disputed() {
    let f = Fixture::funded(true);
    f.client.dispute(&f.depositor);
    assert_eq!(
        f.client.try_release(&f.depositor),
        Err(Ok(Error::EscrowNotFunded))
    );
    f.advance_past_deadline();
    assert_eq!(
        f.client.try_refund(&f.depositor),
        Err(Ok(Error::EscrowNotFunded))
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
fn resolve_guards_against_a_missing_arbiter_even_if_disputed_is_reached() {
    // `dispute` already refuses to run without an arbiter
    // (`dispute_requires_an_arbiter`), so this state is unreachable through
    // the public API. `resolve` re-checks anyway as defense-in-depth --
    // this proves that check actually fires, rather than trusting it's dead
    // code that happens to compile.
    let f = Fixture::funded(false);
    f.env.as_contract(&f.client.address, || {
        let mut escrow = crate::storage::load(&f.env).unwrap();
        escrow.state = State::Disputed;
        crate::storage::save(&f.env, &escrow);
    });

    assert_eq!(
        f.client.try_resolve(&5_000),
        Err(Ok(Error::EscrowNoArbiter))
    );
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
fn resolve_rejects_a_second_call() {
    let f = Fixture::funded(true);
    f.client.dispute(&f.depositor);
    f.client.resolve(&5_000);
    assert_eq!(
        f.client.try_resolve(&5_000),
        Err(Ok(Error::EscrowNotDisputed))
    );
}
