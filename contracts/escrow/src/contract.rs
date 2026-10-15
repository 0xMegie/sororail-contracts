use soroban_sdk::{contract, contractimpl, token, Address, Env};
use sororail_common::{auth, math, Error};

use crate::{
    events, storage,
    types::{Escrow, State},
};

#[contract]
pub struct EscrowContract;

#[contractimpl]
impl EscrowContract {
    /// Configures the escrow. One agreement per deployed instance.
    ///
    /// Requires the depositor's authorization: `init` decides whose funds
    /// `fund` will later pull, so a third party must not be able to name
    /// someone else as depositor.
    ///
    /// `deadline` is a ledger timestamp and must be in the future. After it
    /// passes, the depositor may refund unilaterally.
    pub fn init(
        env: Env,
        depositor: Address,
        beneficiary: Address,
        arbiter: Option<Address>,
        token: Address,
        amount: i128,
        deadline: u64,
    ) -> Result<(), Error> {
        if storage::is_initialized(&env) {
            return Err(Error::AlreadyInitialized);
        }
        math::require_positive(amount)?;
        if deadline <= env.ledger().timestamp() {
            return Err(Error::InvalidTimeRange);
        }
        depositor.require_auth();

        let escrow = Escrow {
            depositor: depositor.clone(),
            beneficiary: beneficiary.clone(),
            arbiter,
            token: token.clone(),
            amount,
            deadline,
            state: State::Created,
        };
        storage::save(&env, &escrow);
        events::Created {
            depositor,
            beneficiary,
            token,
            amount,
            deadline,
        }
        .publish(&env);
        Ok(())
    }

    /// Pulls the agreed amount from the depositor into the contract.
    ///
    /// Rejected once `deadline` has passed (inclusive, matching the boundary
    /// at which the depositor's unilateral refund right begins): funding an
    /// agreement that has already lapsed would only let the depositor pull
    /// their own funds right back out via `refund`, so it is refused up
    /// front with [`Error::DeadlinePassed`] rather than the generic
    /// [`Error::EscrowClosed`], which describes a *terminal* state and would
    /// be misleading here -- a never-funded, lapsed escrow is still `Created`.
    pub fn fund(env: Env) -> Result<(), Error> {
        let mut escrow = storage::load(&env)?;
        if escrow.state.is_terminal() {
            return Err(Error::EscrowClosed);
        }
        auth::require(escrow.state == State::Created, Error::EscrowNotFundable)?;
        if env.ledger().timestamp() >= escrow.deadline {
            return Err(Error::DeadlinePassed);
        }
        escrow.depositor.require_auth();

        escrow.state = State::Funded;
        storage::save(&env, &escrow);

        token::TokenClient::new(&env, &escrow.token).transfer(
            &escrow.depositor,
            env.current_contract_address(),
            &escrow.amount,
        );
        events::Funded {
            depositor: escrow.depositor.clone(),
            amount: escrow.amount,
        }
        .publish(&env);
        Ok(())
    }

    /// Pays the beneficiary. Callable by the depositor or the arbiter.
    ///
    /// The beneficiary cannot release to themselves -- that is the whole point
    /// of the escrow.
    pub fn release(env: Env, caller: Address) -> Result<(), Error> {
        let mut escrow = storage::load(&env)?;
        Self::require_funded(&escrow)?;
        auth::require_auth_either_opt(&caller, &escrow.depositor, &escrow.arbiter)?;

        escrow.state = State::Released;
        storage::save(&env, &escrow);

        Self::pay(&env, &escrow, &escrow.beneficiary, escrow.amount);
        events::Released {
            beneficiary: escrow.beneficiary.clone(),
            amount: escrow.amount,
            released_by: caller,
        }
        .publish(&env);
        Ok(())
    }

    /// Returns the funds to the depositor.
    ///
    /// The depositor may do this only once the deadline has passed. The
    /// arbiter may do it at any time.
    pub fn refund(env: Env, caller: Address) -> Result<(), Error> {
        let mut escrow = storage::load(&env)?;
        Self::require_funded(&escrow)?;
        auth::require_auth_either_opt(&caller, &escrow.depositor, &escrow.arbiter)?;

        let is_arbiter = escrow.arbiter.as_ref().is_some_and(|a| *a == caller);
        if !is_arbiter && env.ledger().timestamp() < escrow.deadline {
            return Err(Error::DeadlineNotReached);
        }

        escrow.state = State::Refunded;
        storage::save(&env, &escrow);

        Self::pay(&env, &escrow, &escrow.depositor, escrow.amount);
        events::Refunded {
            depositor: escrow.depositor.clone(),
            amount: escrow.amount,
            refunded_by: caller,
        }
        .publish(&env);
        Ok(())
    }

    /// Freezes the escrow pending an arbiter decision.
    ///
    /// Callable by either party. Requires that an arbiter was configured --
    /// without one there would be nobody able to resolve the dispute, so the
    /// funds would be stuck.
    pub fn dispute(env: Env, caller: Address) -> Result<(), Error> {
        let mut escrow = storage::load(&env)?;
        auth::require(escrow.arbiter.is_some(), Error::EscrowNoArbiter)?;
        if escrow.state == State::Disputed {
            return Err(Error::EscrowAlreadyDisputed);
        }
        Self::require_funded(&escrow)?;
        auth::require_auth_either(&caller, &escrow.depositor, &escrow.beneficiary)?;

        escrow.state = State::Disputed;
        storage::save(&env, &escrow);
        events::Disputed { raised_by: caller }.publish(&env);
        Ok(())
    }

    /// Splits the funds between the parties. Arbiter only.
    ///
    /// `split_bps` is the share paid to the **beneficiary**, in basis points;
    /// the remainder goes to the depositor. The remainder is computed by
    /// subtraction, so the two payments always sum to exactly the escrowed
    /// amount with no rounding leakage.
    ///
    /// `arbiter` is checked again here even though `dispute` already refused
    /// to reach `Disputed` without one (`arbiter` is set once at `init` and
    /// never cleared, so this can't currently be reached any other way).
    /// Defense-in-depth: if that invariant were ever weakened, this is what
    /// turns a missing arbiter into [`Error::EscrowNoArbiter`] instead of
    /// a call to `require_auth` on an address that was never meant to act.
    pub fn resolve(env: Env, split_bps: u32) -> Result<(), Error> {
        let mut escrow = storage::load(&env)?;
        auth::require(escrow.state == State::Disputed, Error::EscrowNotDisputed)?;
        let arbiter = escrow.arbiter.clone().ok_or(Error::EscrowNoArbiter)?;
        arbiter.require_auth();

        let (to_beneficiary, to_depositor) = math::split_bps(escrow.amount, split_bps)?;

        escrow.state = State::Resolved;
        storage::save(&env, &escrow);

        if to_beneficiary > 0 {
            Self::pay(&env, &escrow, &escrow.beneficiary, to_beneficiary);
        }
        if to_depositor > 0 {
            Self::pay(&env, &escrow, &escrow.depositor, to_depositor);
        }
        events::Resolved {
            arbiter,
            split_bps,
            to_beneficiary,
            to_depositor,
        }
        .publish(&env);
        Ok(())
    }

    /// The current lifecycle position.
    pub fn state(env: Env) -> Result<State, Error> {
        Ok(storage::load(&env)?.state)
    }

    /// The full agreement.
    pub fn get(env: Env) -> Result<Escrow, Error> {
        storage::load(&env)
    }
}

// Helpers. Deliberately outside `#[contractimpl]` so they do not become
// contract entry points.
impl EscrowContract {
    /// Errors unless the escrow is holding funds and still open.
    fn require_funded(escrow: &Escrow) -> Result<(), Error> {
        if escrow.state.is_terminal() {
            return Err(Error::EscrowClosed);
        }
        auth::require(escrow.state == State::Funded, Error::EscrowNotFunded)
    }

    /// Sends `amount` of the escrowed token from this contract to `to`.
    ///
    /// Always called after the state transition has been persisted, so a
    /// reentrant token cannot observe a stale state.
    fn pay(env: &Env, escrow: &Escrow, to: &Address, amount: i128) {
        token::TokenClient::new(env, &escrow.token).transfer(
            &env.current_contract_address(),
            to,
            &amount,
        );
    }
}
