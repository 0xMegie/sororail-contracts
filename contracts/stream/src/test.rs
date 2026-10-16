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
fn accrual_is_linear_between_start_and]uóm­¢G§²ÚîÆ­yÜÍµ…±±•ÍÐÕ¹¥Ð€¡”¹œ¸°ÍÑÉ½½ÁÌ(€€€€¼¼¼™½Èa14°½ÈÑ¡”‰…Í”Õ¹¥Ð‘•™¥¹•‰äÑ¡”Ñ½­•¸Ì‘•¥µ…±Ì¤¸(€€€ÁÕˆÉ…Ñ•}Á•É}Í•½¹è¤ÄÈà°(€€€€¼¼¼1•‘•ÈÑ¥µ•ÍÑ…µÀ…ÐÝ¡¥ …ÉÕ…°‰•¥¹Ì°¥¸Í•½¹‘ÌÍ¥¹”Ñ¡”U¹¥à(€€€€¼¼¼•Á½ ¸(€€€ÁÕˆÍÑ…ÉÐèÔØÐ°(€€€€¼¼¼1•‘•ÈÑ¥µ•ÍÑ…µÀ…ÐÝ¡¥ …ÉÕ…°•¹‘Ì°¥¸Í•½¹‘ÌÍ¥¹”Ñ¡”U¹¥à(€€€€¼¼¼•Á½ ¸(€€€ÁÕˆÍÑ½ÀèÔØÐ°(€€€€¼¼¼]¡•Ñ¡•ÈÑ¡”Í•¹‘•Èµ…ä…¹•°‰•™½É”ÍÑ½Á€¸(€€€ÁÕˆ…¹•±±…‰±”è‰½½°°(€€€€¼¼¼Q½Ñ…°•Ù•ÈÁÕ±±•™É½´Ñ¡”Í•¹‘•È°¥¸Ñ¡”Ñ½­•¸ÌÍµ…±±•ÍÐÕ¹¥Ð¸±Ý…åÌ(€€€€¼¼¼É…Ñ•}Á•É}Í•½¹€¨€¡ÍÑ½À€´ÍÑ…ÉÐ¥€¸(€€€ÁÕˆ‘•Á½Í¥Ñ•è¤ÄÈà°(€€€€¼¼¼Q½Ñ…°•Ù•ÈÁ…¥½ÕÐÑ¼Ñ¡”É•¥Á¥•¹Ð°¥¸Ñ¡”Ñ½­•¸ÌÍµ…±±•ÍÐÕ¹¥Ð¸(€€€ÁÕˆÝ¥Ñ¡‘É…Ý¸è¤ÄÈà°(€€€€¼¼¼Q½Ñ…°•Ù•ÈÉ•ÑÕÉ¹•Ñ¼Ñ¡”Í•¹‘•È½¸…¹•±±…Ñ¥½¸°¥¸Ñ¡”Ñ½­•¸Ì(€€€€¼¼¼Íµ…±±•ÍÐÕ¹¥Ð¸(€€€ÁÕˆÉ•™Õ¹‘•è¤ÄÈà°(€€€€¼¼¼]¡•¸Ñ¡”ÍÑÉ•…´Ý…Ì…¹•±±•°¥˜¥ÐÝ…Ì°¥¸Í•½¹‘ÌÍ¥¹”Ñ¡”U¹¥à(€€€€¼¼¼•Á½ ¸ÉÕ…°ÍÑ½ÁÌ¡•É”¸(€€€ÁÕˆ…¹•±±•‘}…Ðè=ÁÑ¥½¸ñÔØÐø°)ô()¥µÁ°MÑÉ•…´ì(€€€€¼¼¼Q½Ñ…°…ÉÕ•Ñ¼Ñ¡”É•¥Á¥•¹Ð…Ì½˜…Ñ€°Ý¡•Ñ¡•ÈÝ¥Ñ¡‘É…Ý¸½È¹½Ð¸(€€€€¼¼¼(€€€€¼¼¼AÕÉ”è¹¼ÍÑ½É…”°¹¼•¹Ù¥É½¹µ•¹Ð¸±…µÁ•…Ð‰½Ñ •¹‘Ì€´´¹½Ñ¡¥¹œ(€€€€¼¼¼…ÉÕ•Ì‰•™½É”ÍÑ…ÉÑ€°…¹…ÉÕ…°¡…±ÑÌ…ÐÍÑ½Á€½È…ÐÑ¡”(€€€€¼¼¼…¹•±±…Ñ¥½¸Ñ¥µ•ÍÑ…µÀ°Ý¡¥¡•Ù•È½µ•Ì™¥ÉÍÐ¸Q¡”É•ÍÕ±Ð…¸¹•Ù•È(€€€€¼¼¼•á••‘•Á½Í¥Ñ•‘€¸(€€€ÁÕˆ™¸…ÉÕ•‘}…Ð ™Í•±˜°…ÐèÔØÐ¤€´øI•ÍÕ±Ðñ¤ÄÈà°ÉÉ½Èøì(€€€€€€€±•ÐµÕÐ•™™•Ñ¥Ù”€ô…Ðì(€€€€€€€¥˜±•ÐM½µ”¡…¹•±±•¤€ôÍ•±˜¹…¹•±±•‘}…Ðì(€€€€€€€€€€€•™™•Ñ¥Ù”€ô•™™•Ñ¥Ù”¹µ¥¸¡…¹•±±•¤ì(€€€€€€€ô(€€€€€€€•™™•Ñ¥Ù”€ô•™™•Ñ¥Ù”¹µ¥¸¡Í•±˜¹ÍÑ½À¤ì((€€€€€€€¥˜•™™•Ñ¥Ù”€ðôÍ•±˜¹ÍÑ…ÉÐì(€€€€€€€€€€€É•ÑÕÉ¸=¬ À¤ì(€€€€€€€ô(€€€€€€€±•Ð•±…ÁÍ•€ôµ…Ñ èéÍÕˆ¡¤ÄÈàèé™É½´¡•™™•Ñ¥Ù”¤°¤ÄÈàèé™É½´¡Í•±˜¹ÍÑ…ÉÐ¤¤üì(€€€€€€€±•Ð…ÉÕ•€ôµ…Ñ èéµÕ°¡Í•±˜¹É…Ñ•}Á•É}Í•½¹°•±…ÁÍ•¤üì(€€€€€€€=¬¡µ…Ñ èéµ¥¸¡…ÉÕ•°Í•±˜¹‘•Á½Í¥Ñ•¤¤(€€€ô((€€€€¼¼¼ÉÕ•‰ÕÐ¹½Ðå•ÐÝ¥Ñ¡‘É…Ý¸°…Ì½˜…Ñ€¸]¡…ÐÑ¡”É•¥Á¥•¹Ð…¸Ñ…­”¸(€€€ÁÕˆ™¸…Ù…¥±…‰±•}…Ð ™Í•±˜°…ÐèÔØÐ¤€´øI•ÍÕ±Ðñ¤ÄÈà°ÉÉ½Èøì(€€€€€€€±•Ð…ÉÕ•€ôÍ•±˜¹…ÉÕ•‘}…Ð¡…Ð¤üì(€€€€€€€¥˜Í•±˜¹Ý¥Ñ¡‘É…Ý¸€ø…ÉÕ•ì(€€€€€€€€€€€É•ÑÕÉ¸ÉÈ¡ÉÉ½ÈèéU¹‘•É™±½Ü¤ì(€€€€€€€ô(€€€€€€€µ…Ñ èéÍÕˆ¡…ÉÕ•°Í•±˜¹Ý¥Ñ¡‘É…Ý¸¤(€€€ô((€€€€¼¼¼]¡…ÐÑ¡”½¹ÑÉ…ÐÍÑ¥±°¡½±‘Ì™½ÈÑ¡¥ÌÍÑÉ•…´¸(€€€ÁÕˆ™¸É•µ…¥¹¥¹œ ™Í•±˜¤€´øI•ÍÕ±Ðñ¤ÄÈà°ÉÉ½Èøì(€€€€€€€µ…Ñ èéÍÕˆ¡µ…Ñ èéÍÕˆ¡Í•±˜¹‘•Á½Í¥Ñ•°Í•±˜¹Ý¥Ñ¡‘É…Ý¸¤ü°Í•±˜¹É•™Õ¹‘•¤(€€€ô((€€€€¼¼¼]¡•Ñ¡•ÈÑ¡”ÍÑÉ•…´¡…Ì‰••¸…¹•±±•¸(€€€€¼¼¼(€€€€¼¼¼Q¡”±¥™•å±”¥Ì‰¥¹…Éä…¹½¹”µÝ…ä°Í¼¥Ð¥Ìµ½‘•±±•‰äÑ¡”(€€€€¼¼¼…¹•±±•‘}…Ñ€Ñ¥µ•ÍÑ…µÀ€¡Ý¡¥ …ÉÕ…°¹••‘Ì…¹åÝ…ä¤É…Ñ¡•ÈÑ¡…¸„(€€€€¼¼¼MÑ…Ñ•€•¹Õ´±¥­”•ÍÉ½ÜÌ¸M•”€‰1¥™•å±”µ½‘•±¥¹œˆ¥¸MA¹µ¸(€€€ÁÕˆ™¸¥Í}…¹•±±• ™Í•±˜¤€´ø‰½½°ì(€€€€€€€Í•±˜¹…¹•±±•‘}…Ð¹¥Í}Í½µ” ¤(€€€ô((€€€€¼¼¼Q¡”‘•±…É•ÍÁ…¸¥¸Í•½¹‘Ì¸(€€€ÁÕˆ™¸‘ÕÉ…Ñ¥½¸ ™Í•±˜¤€´øÔØÐì(€€€€€€€Í•±˜¹ÍÑ½À¹Í…ÑÕÉ…Ñ¥¹}ÍÕˆ¡Í•±˜¹ÍÑ…ÉÐ¤(€€€ô)ô(