use soroban_sdk::{contract, contractimpl, token, Address, Env};
use sororail_common::{auth, math, Error};

use crate::{events, storage, types::Stream};

#[contract]
pub struct StreamContract;

#[contractimpl]
impl StreamContract {
    /// Creates and fully funds a stream.
    ///
    /// The whole span is funded up front: `rate_per_second * (stop - start)`
    /// is pulled from the sender immediately. A stream is never partially
    /// funded, so the recipient can rely on what it promises.
    ///
    /// `start` may be in the past, which backdates accrual -- the sender is
    /// choosing to make funds immediately withdrawable.
    ///
    /// `sender` and `recipient` must differ; a self-stream is rejected.
    ///
    /// # Why positional arguments (not a `CreateStreamParams` struct)
    ///
    /// Soroban does support a single `#[contracttype]` struct parameter, and
    /// that would silence this lint. Positional arguments were kept anyway so
    /// the on-chain ABI stays a flat argument list — the same shape the
    /// TypeScript SDK's uniform build → simulate → sign → send → confirm
    /// surface (SPEC.md) will bind without introducing a nested params object
    /// per entry point. Revisit only if the SDK deliberately adopts struct
    /// params across every contract.
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        env: Env,
        sender: Address,
        recipient: Address,
        token: Address,
        rate_per_second: i128,
        start: u64,
        stop: u64,
        cancellable: bool,
    ) -> Result<(), Error> {
        if storage::is_initialized(&env) {
            return Err(Error::AlreadyInitialized);
        }
        if sender == recipient {
            return Err(Error::IdenticalParties);
        }
        math::require_positive(rate_per_second)?;
        if stop <= start {
            return Err(Error::InvalidTimeRange);
        }
        let deposited = math::mul(
            rate_per_second,
            math::sub(i128::from(stop), i128::from(start))?,
        )?;

        sender.require_auth();

        let stream = Stream {
            sender: sender.clone(),
            recipient: recipient.clone(),
            token: token.clone(),
            rate_per_second,
            start,
            stop,
            cancellable,
            deposited,
            withdrawn: 0,
            refunded: 0,
            cancelled_at: None,
        };
        storage::save(&env, &stream);

        Self::pull(&env, &stream, deposited);
        events::Created {
            sender,
            recipient,
            token,
            rate_per_second,
            start,
            stop,
            cancellable,
            deposited,
        }
        .publish(&env);
        Ok(())
    }

    /// Withdraws accrued funds to the recipient.
    ///
    /// `None` withdraws everything currently available. Returns the amount
    /// actually withdrawn.
    pub fn withdraw(env: Env, amount: Option<i128>) -> Result<i128, Error> {
        let mut stream = storage::load(&env)?;
        stream.recipient.require_auth();

        let available = stream.available_at(env.ledger().timestamp())?;
        let requested = amount.unwrap_or(available);
        math::require_positive(requested)?;
        if requested > available {
            return Err(Error::StreamInsufficientAccrued);
        }

        stream.withdrawn = math::add(stream.withdrawn, requested)?;
        storage::save(&env, &stream);

        Self::pay(&env, &stream, &stream.recipient, requested);
        events::Withdrawn {
            recipient: stream.recipient.clone(),
            amount: requested,
            total_withdrawn: stream.withdrawn,
        }
        .publish(&env);
        Ok(requested)
    }

    /// Cancels the stream: settles what has accrued to the recipient and
    /// returns the rest to the sender. Sender only, and only if the stream was
    /// created `cancellable`.
    ///
    /// Settlement is immediate and complete, so nothing is left for the
    /// recipient to withdraw afterwards.
    pub fn cancel(env: Env) -> Result<(), Error> {
        let mut stream = storage::load(&env)?;
        if stream.is_cancelled() {
            return Err(Error::StreamCancelled);
        }
        auth::require(stream.cancellable, Error::StreamNotCancellable)?;
        stream.sender.require_auth();

        let now = env.ledger().timestamp();
        let settled = stream.available_at(now)?;
        stream.withdrawn = math::add(stream.withdrawn, settled)?;
        let refund = math::sub(stream.deposited, stream.withdrawn)?;
        stream.refunded = refund;
        stream.cancelled_at = Some(now);
        storage::save(&env, &stream);

        if settled > 0 {
            Self::pay(&env, &stream, &stream.recipient, settled);
        }
        if refund > 0 {
            Self::pay(&env, &stream, &stream.sender, refund);
        }
        events::Cancelled {
            sender: stream.sender.clone(),
            settled_to_recipient: settled,
            refunded_to_sender: refund,
            cancelled_at: now,
        }
        .publish(&env);
        Ok(())
    }

    /// Adds funds, extending `stop` by the span they buy.
    ///
    /// `amount` must be an exact multiple of `rate_per_second`. A partial
    /// second cannot be represented without breaking the funding invariant
    /// (`deposited == rate * (stop - start)`), and silently keeping the dust
    /// would strand it, so an inexact amount is rejected instead.
    pub fn top_up(env: Env, amount: i128) -> Result<(), Error> {
        let mut stream = storage::load(&env)?;
        if stream.is_cancelled() {
            return Err(Error::StreamCancelled);
        }
        stream.sender.require_auth();
        math::require_positive(amount)?;
        if amount.checked_rem(stream.rate_per_second) != Some(0) {
            return Err(Error::InvalidAmount);
        }

        let seconds = u64::try_from(math::div(amount, stream.rate_per_second)?)
            .map_err(|_| Error::InvalidTimeRange)?;
        stream.stop = stream
            .stop
            .checked_add(seconds)
            .ok_or(Error::InvalidTimeRange)?;
        stream.deposited = math::add(stream.deposited, amount)?;
        storage::save(&env, &stream);

        Self::pull(&env, &stream, amount);
        events::ToppedUp {
            sender: stream.sender.clone(),
            amount,
            new_stop: stream.stop,
            deposited: stream.deposited,
        }
        .publish(&env);
        Ok(())
    }

    /// Extends `stop`, pulling the additional funds the longer span requires.
    pub fn extend(env: Env, new_stop: u64) -> Result<(), Error> {
        let mut stream = storage::load(&env)?;
        if stream.is_cancelled() {
            return Err(Error::StreamCancelled);
        }
        stream.sender.require_auth();
        if new_stop <= stream.stop {
            return Err(Error::StreamNotExtendable);
        }

        let added = math::mul(
            stream.rate_per_second,
            math::sub(i128::from(new_stop), i128::from(stream.stop))?,
        )?;
        stream.stop = new_stop;
        stream.deposited = math::add(stream.deposited, added)?;
        storage::save(&env, &stream);

        Self::pull(&env, &stream, added);
        events::Extended {
            sender: stream.sender.clone(),
            added,
            new_stop,
            deposited: stream.deposited,
        }
        .publish(&env);
        Ok(())
    }

    /// Accrued but unwithdrawn for the recipient; refundable remainder for the
    /// sender; zero for anyone else.
    pub fn balance_of(env: Env, who: Address) -> Result<i128, Error> {
        let stream = storage::load_view(&env)?;
        let now = env.ledger().timestamp();

        if who == stream.recipient {
            return stream.available_at(now);
        }
        if who == stream.sender {
            let unaccrued = math::sub(stream.deposited, stream.accrued_at(now)?)?;
            return math::sub(unaccrued, stream.refunded);
        }
        Ok(0)
    }

    /// What the contract still holds for this stream.
    pub fn remaining(env: Env) -> Result<i128, Error> {
        storage::load_view(&env)?.remaining()
    }

    /// Extends the instance TTL for a long-lived stream without changing it.
    pub fn bump(env: Env) -> Result<(), Error> {
        storage::load_view(&env)?;
        sororail_common::storage::extend_instance(&env);
        Ok(())
    }

    /// The full stream record.
    pub fn get(env: Env) -> Result<Stream, Error> {
        storage::load_view(&env)
    }
}

// Helpers. Outside `#[contractimpl]` so they are not contract entry points.
impl StreamContract {
    /// Pulls `amount` of the streamed token from the sender into this contract.
    fn pull(env: &Env, stream: &Stream, amount: i128) {
        debug_assert!(amount > 0, "pull requires a positive amount");
        token::TokenClient::new(env, &stream.token).transfer(
            &stream.sender,
            env.current_contract_address(),
            &amount,
        );
    }

    /// Sends `amount` of the streamed token from this contract to `to`.
    ///
    /// Always called after the state change has been persisted.
    fn pay(env: &Env, stream: &Stream, to: &Address, amount: i128) {
        debug_assert!(amount > 0, "pay requires a positive amount");
        token::TokenClient::new(env, &stream.token).transfer(
            &env.current_contract_address(),
            to,
            &amount,
        );
    }
}
