// Test fixtures do plain arithmetic on known-small constants; the checked-math
// rule is for contract code.
#![allow(clippy::arithmetic_side_effects)]

use soroban_sdk::{
    map,
    testutils::{storage::Instance as _, Address as _, Events as _, Ledger as _},
    token::{StellarAssetClient, TokenClient},
    vec, Address, Env, Event, IntoVal, Symbol, Val,
};
use sororail_common::{storage::INSTANCE_BUMP, testutils::TestEnv, Error};

use crate::{
    contract::{StreamContract, StreamContractClient},
    events::{Cancelled, Created, Withdrawn},
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
fn accrual_is_linear_between_start_and]uóm­¢G§²ÚîÆ­yÜÍµ…±±•ÍĞÕ¹¥Ğ€¡”¹œ¸°ÍÑÉ½½ÁÌ(€€€€¼¼¼™½Èa14°½ÈÑ¡”‰…Í”Õ¹¥Ğ‘•™¥¹•‰äÑ¡”Ñ½­•¸Ì‘•¥µ…±Ì¤¸(€€€ÁÕˆÉ…Ñ•}Á•É}Í•½¹è¤ÄÈà°(€€€€¼¼¼1•‘•ÈÑ¥µ•ÍÑ…µÀ…Ğİ¡¥ …ÉÕ…°‰•¥¹Ì°¥¸Í•½¹‘ÌÍ¥¹”Ñ¡”U¹¥à(€€€€¼¼¼•Á½ ¸(€€€ÁÕˆÍÑ…ÉĞèÔØĞ°(€€€€¼¼¼1•‘•ÈÑ¥µ•ÍÑ…µÀ…Ğİ¡¥ …ÉÕ…°•¹‘Ì°¥¸Í•½¹‘ÌÍ¥¹”Ñ¡”U¹¥à(€€€€¼¼¼•Á½ ¸(€€€ÁÕˆÍÑ½ÀèÔØĞ°(€€€€¼¼¼]¡•Ñ¡•ÈÑ¡”Í•¹‘•Èµ…ä…¹•°‰•™½É”ÍÑ½Á€¸(€€€ÁÕˆ…¹•±±…‰±”è‰½½°°(€€€€¼¼¼Q½Ñ…°•Ù•ÈÁÕ±±•™É½´Ñ¡”Í•¹‘•È°¥¸Ñ¡”Ñ½­•¸ÌÍµ…±±•ÍĞÕ¹¥Ğ¸±İ…åÌ(€€€€¼¼¼É…Ñ•}Á•É}Í•½¹€¨€¡ÍÑ½À€´ÍÑ…ÉĞ¥€¸(€€€ÁÕˆ‘•Á½Í¥Ñ•è¤ÄÈà°(€€€€¼¼¼Q½Ñ…°•Ù•ÈÁ…¥½ÕĞÑ¼Ñ¡”É•¥Á¥•¹Ğ°¥¸Ñ¡”Ñ½­•¸ÌÍµ…±±•ÍĞÕ¹¥Ğ¸(€€€ÁÕˆİ¥Ñ¡‘É…İ¸è¤ÄÈà°(€€€€¼¼¼Q½Ñ…°•Ù•ÈÉ•ÑÕÉ¹•Ñ¼Ñ¡”Í•¹‘•È½¸…¹•±±…Ñ¥½¸°¥¸Ñ¡”Ñ½­•¸Ì(€€€€¼¼¼Íµ…±±•ÍĞÕ¹¥Ğ¸(€€€ÁÕˆÉ•™Õ¹‘•è¤ÄÈà°(€€€€¼¼¼]¡•¸Ñ¡”ÍÑÉ•…´İ…Ì…¹•±±•°¥˜¥Ğİ…Ì°¥¸Í•½¹‘ÌÍ¥¹”Ñ¡”U¹¥à(€€€€¼¼¼•Á½ ¸ÉÕ…°ÍÑ½ÁÌ¡•É”¸(€€€ÁÕˆ…¹•±±•‘}…Ğè=ÁÑ¥½¸ñÔØĞø°)ô()¥µÁ°MÑÉ•…´ì(€€€€¼¼¼Q½Ñ…°…ÉÕ•Ñ¼Ñ¡”É•¥Á¥•¹Ğ…Ì½˜…Ñ€°İ¡•Ñ¡•Èİ¥Ñ¡‘É…İ¸½È¹½Ğ¸(€€€€¼¼¼(€€€€¼¼¼AÕÉ”è¹¼ÍÑ½É…”°¹¼•¹Ù¥É½¹µ•¹Ğ¸±…µÁ•…Ğ‰½Ñ •¹‘Ì€´´¹½Ñ¡¥¹œ(€€€€¼¼¼…ÉÕ•Ì‰•™½É”ÍÑ…ÉÑ€°…¹…ÉÕ…°¡…±ÑÌ…ĞÍÑ½Á€½È…ĞÑ¡”(€€€€¼¼¼…¹•±±…Ñ¥½¸Ñ¥µ•ÍÑ…µÀ°İ¡¥¡•Ù•È½µ•Ì™¥ÉÍĞ¸Q¡”É•ÍÕ±Ğ…¸¹•Ù•È(€€€€¼¼¼•á••‘•Á½Í¥Ñ•‘€¸(€€€ÁÕˆ™¸…ÉÕ•‘}…Ğ ™Í•±˜°…ĞèÔØĞ¤€´øI•ÍÕ±Ğñ¤ÄÈà°ÉÉ½Èøì(€€€€€€€±•ĞµÕĞ•™™•Ñ¥Ù”€ô…Ğì(€€€€€€€¥˜±•ĞM½µ”¡…¹•±±•¤€ôÍ•±˜¹…¹•±±•‘}…Ğì(€€€€€€€€€€€•™™•Ñ¥Ù”€ô•™™•Ñ¥Ù”¹µ¥¸¡…¹•±±•¤ì(€€€€€€€ô(€€€€€€€•™™•Ñ¥Ù”€ô•™™•Ñ¥Ù”¹µ¥¸¡Í•±˜¹ÍÑ½À¤ì((€€€€€€€¥˜•™™•Ñ¥Ù”€ğôÍ•±˜¹ÍÑ…ÉĞì(€€€€€€€€€€€É•ÑÕÉ¸=¬ À¤ì(€€€€€€€ô(€€€€€€€±•Ğ•±…ÁÍ•€ôµ…Ñ èéÍÕˆ¡¤ÄÈàèé™É½´¡•™™•Ñ¥Ù”¤°¤ÄÈàèé™É½´¡Í•±˜¹ÍÑ…ÉĞ¤¤üì(€€€€€€€±•Ğ…ÉÕ•€ôµ…Ñ èéµÕ°¡Í•±˜¹É…Ñ•}Á•É}Í•½¹°•±…ÁÍ•¤üì(€€€€€€€=¬¡µ…Ñ èéµ¥¸¡…ÉÕ•°Í•±˜¹‘•Á½Í¥Ñ•¤¤(€€€ô((€€€€¼¼¼ÉÕ•‰ÕĞ¹½Ğå•Ğİ¥Ñ¡‘É…İ¸°…Ì½˜…Ñ€¸]¡…ĞÑ¡”É•¥Á¥•¹Ğ…¸Ñ…­”¸(€€€ÁÕˆ™¸…Ù…¥±…‰±•}…Ğ ™Í•±˜°…ĞèÔØĞ¤€´øI•ÍÕ±Ğñ¤ÄÈà°ÉÉ½Èøì(€€€€€€€±•Ğ…ÉÕ•€ôÍ•±˜¹…ÉÕ•‘}…Ğ¡…Ğ¤üì(€€€€€€€¥˜Í•±˜¹İ¥Ñ¡‘É…İ¸€ø…ÉÕ•ì(€€€€€€€€€€€É•ÑÕÉ¸ÉÈ¡ÉÉ½ÈèéU¹‘•É™±½Ü¤ì(€€€€€€€ô(€€€€€€€µ…Ñ èéÍÕˆ¡…ÉÕ•°Í•±˜¹İ¥Ñ¡‘É…İ¸¤(€€€ô((€€€€¼¼¼]¡…ĞÑ¡”½¹ÑÉ…ĞÍÑ¥±°¡½±‘Ì™½ÈÑ¡¥ÌÍÑÉ•…´¸(€€€ÁÕˆ™¸É•µ…¥¹¥¹œ ™Í•±˜¤€´øI•ÍÕ±Ğñ¤ÄÈà°ÉÉ½Èøì(€€€€€€€µ…Ñ èéÍÕˆ¡µ…Ñ èéÍÕˆ¡Í•±˜¹‘•Á½Í¥Ñ•°Í•±˜¹İ¥Ñ¡‘É…İ¸¤ü°Í•±˜¹É•™Õ¹‘•¤(€€€ô((€€€€¼¼¼]¡•Ñ¡•ÈÑ¡”ÍÑÉ•…´¡…Ì‰••¸…¹•±±•¸(€€€€¼¼¼(€€€€¼¼¼Q¡”±¥™•å±”¥Ì‰¥¹…Éä…¹½¹”µİ…ä°Í¼¥Ğ¥Ìµ½‘•±±•‰äÑ¡”(€€€€¼¼¼…¹•±±•‘}…Ñ€Ñ¥µ•ÍÑ…µÀ€¡İ¡¥ …ÉÕ…°¹••‘Ì…¹åİ…ä¤É…Ñ¡•ÈÑ¡…¸„(€€€€¼¼¼MÑ…Ñ•€•¹Õ´±¥­”•ÍÉ½ÜÌ¸M•”€‰1¥™•å±”µ½‘•±¥¹œˆ¥¸MA¹µ¸(€€€ÁÕˆ™¸¥Í}…¹•±±• ™Í•±˜¤€´ø‰½½°ì(€€€€€€€Í•±˜¹…¹•±±•‘}…Ğ¹¥Í}Í½µ” ¤(€€€ô((€€€€¼¼¼Q¡”‘•±…É•ÍÁ…¸¥¸Í•½¹‘Ì¸(€€€ÁÕˆ™¸‘ÕÉ…Ñ¥½¸ ™Í•±˜¤€´øÔØĞì(€€€€€€€Í•±˜¹ÍÑ½À¹Í…ÑÕÉ…Ñ¥¹}ÍÕˆ¡Í•±˜¹ÍÑ…ÉĞ¤(€€€ô)ô(
// ------------------------------------------------ issues #244-#247

#[test]
fn balance_of_an_unrelated_address_is_zero() {
    let f = Fixture::new(true);
    assert_eq!(f.client.balance_of(&f.outsider), 0);
    f.at(START + DURATION / 2);
    assert_eq!(f.client.balance_of(&f.outsider), 0);
    f.at(STOP);
    assert_eq!(f.client.try_balance_of(&f.outsider), Ok(Ok(0)));
}

#[test]
fn remaining_is_zero_after_full_withdrawal() {
    let f = Fixture::new(false);
    f.at(STOP);
    assert_eq!(f.client.withdraw(&None), DEPOSITED);
    assert_eq!(f.client.remaining(), 0);
    f.assert_conserved();
}

#[test]
fn duration_is_one_when_stop_is_start_plus_one() {
    let env = Env::default();
    let mut s = bare(&env);
    s.stop = s.start + 1;
    assert_eq!(s.duration(), 1);
}

#[test]
fn is_cancelled_flips_exactly_when_cancel_is_recorded() {
    let env = Env::default();
    let mut s = bare(&env);
    assert!(!s.is_cancelled());
    s.cancelled_at = Some(START + 1);
    assert!(s.is_cancelled());

    let f = Fixture::new(true);
    assert!(!f.client.get().is_cancelled());
    f.at(START + 1);
    f.client.cancel();
    assert!(f.client.get().is_cancelled());
}
