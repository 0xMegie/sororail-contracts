use soroban_sdk::{contract, contractimpl, token, Address, Env, Vec};
use sororail_common::{math, Error};

use crate::{
    events,
    types::{Payment, Receipt, MAX_RECIPIENTS},
};

#[contract]
pub struct BatchPayoutContract;

#[contractimpl]
impl BatchPayoutContract {
    /// Pays each recipient their own amount, in one transaction.
    ///
    /// All-or-nothing: if any transfer fails, the host reverts the entire
    /// invocation including the transfers already made. Nobody is paid unless
    /// everybody is.
    ///
    /// The whole batch is validated and totalled **before** any money moves,
    /// so an invalid line is rejected without a partial payout ever being
    /// attempted.
    pub fn execute(
        env: Env,
        funder: Address,
        token: Address,
        recipients: Vec<Payment>,
    ) -> Result<Receipt, Error> {
        let count = Self::check_size(&recipients)?;
        let contract_address = env.current_contract_address();

        let mut total: i128 = 0;
        for payment in recipients.iter() {
            if payment.to == contract_address {
                return Err(Error::IdenticalParties);
            }
            math::require_positive(payment.amount)?;
            total = math::add(total, payment.amount)?;
        }

        funder.require_auth();

        let client = token::TokenClient::new(&env, &token);
        for payment in recipients.iter() {
            client.transfer(&funder, &payment.to, &payment.amount);
        }

        let receipt = Receipt { count, total };
        events::Executed {
            funder,
            token,
            count,
            total,
        }
        .publish(&env);
        Ok(receipt)
    }

    /// Pays every recipient the same amount, in one transaction.
    ///
    /// The common payroll shape, and cheaper to submit than [`Self::execute`]
    /// because the amount crosses the wire once rather than once per line.
    pub fn execute_equal(
        env: Env,
        funder: Address,
        token: Address,
        recipients: Vec<Address>,
        amount_each: i128,
    ) -> Result<Receipt, Error> {
        math::require_positive(amount_each)?;

        let count = Self::check_size_of(recipients.len())?;
        let contract_address = env.current_contract_address();

        for to in recipients.iter() {
            if to == contract_address {
                return Err(Error::IdenticalParties);
            }
        }

        let total = math::mul(amount_each, i128::from(count))?;

        funder.require_auth();

        let client = token::TokenClient::new(&env, &token);
        for to in recipients.iter() {
            client.transfer(&funder, &to, &amount_each);
        }

        let receipt = Receipt { count, total };
        events::Executed {
            funder,
            token,
            count,
            total,
        }
        .publish(&env);
        Ok(receipt)
    }

    /// The maximum recipients one batch may contain.
    ///
    /// Exposed so a client can split an oversized payroll before submitting
    /// rather than discovering the cap by having a transaction rejected.
    pub fn max_recipients(_env: Env) -> u32 {
        MAX_RECIPIENTS
    }

    /// Totals a batch without moving anything, for a pre-signature preview.
    ///
    /// Runs exactly the validation [`Self::execute`] does, so a preview that
    /// succeeds means the batch itself will not be rejected for size, an
    /// invalid amount, or an overflowing total.
    pub fn preview(env: Env, recipients: Vec<Payment>) -> Result<Receipt, Error> {
        let count = Self::check_size(&recipients)?;
        let contract_address = env.current_contract_address();
        let mut total: i128 = 0;
        for payment in recipients.iter() {
            if payment.to == contract_address {
                return Err(Error::IdenticalParties);
            }
            math::require_positive(payment.amount)?;
            total = math::add(total, payment.amount)?;
        }
        Ok(Receipt { count, total })
    }
}

// Helpers. Outside `#[contractimpl]` so they are not contract entry points.
impl BatchPayoutContract {
    fn check_size(recipients: &Vec<Payment>) -> Result<u32, Error> {
        Self::check_size_of(recipients.len())
    }

    fn check_size_of(len: u32) -> Result<u32, Error> {
        if len == 0 {
            return Err(Error::BatchEmpty);
        }
        if len > MAX_RECIPIENTS {
            return Err(Error::BatchTooLarge);
        }
        Ok(len)
    }
}
