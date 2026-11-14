//! Proof-backed fact purchases. Public evidence; no TEE, oracle or optimistic blame.
pub mod proof;

use near_sdk::json_types::U128;
use near_sdk::store::LookupMap;
use near_sdk::{AccountId, Gas, NearToken, PanicOnDefault, Promise, env, near};
use proof::Opening;

const MAX_KEYS: usize = 8;
const MAX_CALL_BYTES: usize = 8192;

#[near(serializers = [borsh, json])]
#[derive(Clone)]
pub struct Dataset {
    pub seller: AccountId,
    pub name: String,
    pub root: String,
    pub source_uri: String,
    pub published_height: u64,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CloseReason {
    Open,
    Fulfilled,
    ResolvedWithAbsence,
    UnacceptedExpired,
    WorkExpired,
    BuyerCancelled,
    SellerClosed,
}

#[near(serializers = [borsh, json])]
#[derive(Clone)]
pub struct Job {
    pub id: u64,
    pub buyer: AccountId,
    pub seller: AccountId,
    pub dataset: String,
    pub root: String,
    pub keys: Vec<String>,
    pub price: U128,
    pub remaining: U128,
    pub paid: U128,
    pub refunded: U128,
    pub accept_deadline: u64,
    pub work_deadline: u64,
    pub accepted: bool,
    pub resolved_mask: u8,
    pub delivered_mask: u8,
    pub close_reason: CloseReason,
}

#[near(serializers = [borsh, json])]
#[derive(Clone)]
pub struct Delivery {
    pub job_id: u64,
    pub index: u8,
    pub key: String,
    pub opening: Opening,
    pub present: bool,
    pub seller_credit: U128,
    pub buyer_refund: U128,
    pub height: u64,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Default)]
pub struct Credit {
    pub available: U128,
    pub in_flight: U128,
    pub nonce: u64,
    pub retained_storage_paid: U128,
}

#[near(serializers = [json])]
pub struct Accounting {
    pub deposits: U128,
    pub locked: U128,
    pub available: U128,
    pub in_flight: U128,
    pub withdrawn: U128,
    pub retained_storage_charges: U128,
    pub conserved: bool,
    pub contract_balance: U128,
    pub current_storage_bytes: u64,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct DataEscrow {
    datasets: LookupMap<(AccountId, String), Dataset>,
    jobs: LookupMap<u64, Job>,
    deliveries: LookupMap<(u64, u8), Delivery>,
    credits: LookupMap<AccountId, Credit>,
    next_job: u64,
    deposits: u128,
    locked: u128,
    available: u128,
    in_flight: u128,
    withdrawn: u128,
    retained_storage_charges: u128,
}

#[near]
impl DataEscrow {
    #[init]
    pub fn new() -> Self {
        Self {
            datasets: LookupMap::new(b"d"),
            jobs: LookupMap::new(b"j"),
            deliveries: LookupMap::new(b"p"),
            credits: LookupMap::new(b"c"),
            next_job: 0,
            deposits: 0,
            locked: 0,
            available: 0,
            in_flight: 0,
            withdrawn: 0,
            retained_storage_charges: 0,
        }
    }

    /// Immutable seller-signed publication; a new root requires a new dataset name.
    #[payable]
    pub fn publish_dataset(&mut self, name: String, root: String, source_uri: String) -> Dataset {
        call_limit();
        assert!(
            !name.is_empty() && name.len() <= 64,
            "dataset name must be 1–64 bytes"
        );
        assert!(proof::valid_hash(&root), "invalid dataset root");
        assert!(source_uri.len() <= 256, "source URI too long");
        let seller = env::predecessor_account_id();
        let key = (seller.clone(), name.clone());
        assert!(
            !self.datasets.contains_key(&key),
            "dataset name already published"
        );
        let before = env::storage_usage();
        let dataset = Dataset {
            seller,
            name,
            root,
            source_uri,
            published_height: env::block_height(),
        };
        self.datasets.insert(key, dataset.clone());
        self.fund(before, 0);
        event("dataset_published", &dataset);
        dataset
    }

    #[payable]
    pub fn open_job(
        &mut self,
        seller: AccountId,
        dataset: String,
        keys: Vec<String>,
        price: U128,
        accept_deadline: u64,
        work_deadline: u64,
    ) -> Job {
        call_limit();
        assert!(
            !keys.is_empty() && keys.len() <= MAX_KEYS,
            "requires 1–8 fact keys"
        );
        for (i, key) in keys.iter().enumerate() {
            assert!(
                proof::valid_key(key) && !keys[..i].contains(key),
                "invalid or duplicate fact key"
            );
        }
        assert!(price.0 > 0, "price must be positive");
        assert!(
            env::block_height() < accept_deadline && accept_deadline < work_deadline,
            "invalid deadlines"
        );
        let published = self
            .datasets
            .get(&(seller.clone(), dataset.clone()))
            .expect("unknown seller dataset")
            .clone();
        let buyer = env::predecessor_account_id();
        assert!(buyer != seller, "buyer and seller must differ");
        let before = env::storage_usage();
        self.ensure_account(&seller);
        self.ensure_account(&buyer);
        let id = self.next_job;
        self.next_job = id.checked_add(1).expect("job id overflow");
        let job = Job {
            id,
            buyer,
            seller,
            dataset,
            root: published.root,
            keys,
            price,
            remaining: price,
            paid: U128(0),
            refunded: U128(0),
            accept_deadline,
            work_deadline,
            accepted: false,
            resolved_mask: 0,
            delivered_mask: 0,
            close_reason: CloseReason::Open,
        };
        self.jobs.insert(id, job.clone());
        self.locked = add(self.locked, price.0);
        self.fund(before, price.0);
        event("job_opened", &job);
        job
    }

    pub fn accept_job(&mut self, job_id: u64) -> Job {
        let before = env::storage_usage();
        let mut job = self.open(job_id);
        assert_eq!(
            env::predecessor_account_id(),
            job.seller,
            "only named seller may accept"
        );
        assert!(
            !job.accepted && env::block_height() <= job.accept_deadline,
            "acceptance expired or already accepted"
        );
        job.accepted = true;
        self.jobs.insert(job_id, job.clone());
        self.no_growth(before);
        event("job_accepted", &job);
        job
    }

    /// Check a requested fact before allocating any job payment. Rejected calls roll back.
    /// An authenticated vacant slot refunds that key; it does not earn a seller fee.
    #[payable]
    pub fn deliver(&mut self, job_id: u64, index: u8, opening: Opening) -> Delivery {
        call_limit();
        let mut job = self.open(job_id);
        assert_eq!(
            env::predecessor_account_id(),
            job.seller,
            "only named seller may deliver"
        );
        assert!(
            job.accepted && env::block_height() <= job.work_deadline,
            "job unaccepted or work expired"
        );
        assert!((index as usize) < job.keys.len(), "fact index out of range");
        let bit = 1u8 << index;
        assert!(job.resolved_mask & bit == 0, "fact already resolved");
        let key = job.keys[index as usize].clone();
        let present =
            proof::verify(&job.root, &key, &opening).unwrap_or_else(|why| env::panic_str(why));
        let before = env::storage_usage();
        let count = job.keys.len() as u128;
        // Every yocto is allocated once. Earlier key indices receive the remainder.
        let share = job.price.0 / count + u128::from(u128::from(index) < job.price.0 % count);
        job.remaining.0 = sub(job.remaining.0, share);
        self.locked = sub(self.locked, share);
        job.resolved_mask |= bit;
        if present {
            job.delivered_mask |= bit;
            job.paid.0 = add(job.paid.0, share);
            self.credit(&job.seller, share);
        } else {
            job.refunded.0 = add(job.refunded.0, share);
            self.credit(&job.buyer, share);
        }
        if u32::from(job.resolved_mask).count_ones() == job.keys.len() as u32 {
            assert_eq!(job.remaining.0, 0, "unallocated remainder");
            job.close_reason = if job.resolved_mask == job.delivered_mask {
                CloseReason::Fulfilled
            } else {
                CloseReason::ResolvedWithAbsence
            };
        }
        let delivery = Delivery {
            job_id,
            index,
            key,
            opening,
            present,
            seller_credit: U128(if present { share } else { 0 }),
            buyer_refund: U128(if present { 0 } else { share }),
            height: env::block_height(),
        };
        self.jobs.insert(job_id, job);
        self.deliveries.insert((job_id, index), delivery.clone());
        self.fund(before, 0);
        event("fact_resolved", &delivery);
        delivery
    }

    /// Permissionless: only an expired unaccepted offer or expired work may close.
    pub fn expire_job(&mut self, job_id: u64) -> Job {
        let job = self.open(job_id);
        let reason = if job.accepted {
            assert!(
                env::block_height() > job.work_deadline,
                "work deadline still open"
            );
            CloseReason::WorkExpired
        } else {
            assert!(
                env::block_height() > job.accept_deadline,
                "acceptance deadline still open"
            );
            CloseReason::UnacceptedExpired
        };
        self.close(job, reason)
    }

    pub fn cancel_job(&mut self, job_id: u64) -> Job {
        let job = self.open(job_id);
        assert_eq!(
            env::predecessor_account_id(),
            job.buyer,
            "only buyer may cancel"
        );
        assert!(!job.accepted, "accepted work cannot be cancelled by buyer");
        self.close(job, CloseReason::BuyerCancelled)
    }

    pub fn close_job(&mut self, job_id: u64) -> Job {
        let job = self.open(job_id);
        assert_eq!(
            env::predecessor_account_id(),
            job.seller,
            "only seller may close early"
        );
        self.close(job, CloseReason::SellerClosed)
    }

    #[payable]
    pub fn withdraw(&mut self, amount: Option<U128>) -> Promise {
        assert_eq!(
            env::attached_deposit().as_yoctonear(),
            1,
            "requires exactly 1 yoctoNEAR"
        );
        assert!(
            env::prepaid_gas() >= Gas::from_tgas(30),
            "withdraw requires 30 Tgas"
        );
        let caller = env::predecessor_account_id();
        let credit = self.credits.get_mut(&caller).expect("no credit account");
        assert_eq!(credit.in_flight.0, 0, "withdrawal already pending");
        credit.available.0 = add(credit.available.0, 1);
        self.available = add(self.available, 1);
        self.deposits = add(self.deposits, 1);
        let amount = amount.unwrap_or(credit.available);
        assert!(
            amount.0 > 0 && amount.0 <= credit.available.0,
            "invalid withdrawal amount"
        );
        credit.available.0 -= amount.0;
        credit.in_flight = amount;
        credit.nonce = credit.nonce.checked_add(1).expect("nonce overflow");
        self.available -= amount.0;
        self.in_flight = add(self.in_flight, amount.0);
        Promise::new(caller.clone())
            .transfer(NearToken::from_yoctonear(amount.0))
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(Gas::from_tgas(15))
                    .on_withdraw(caller, credit.nonce, amount),
            )
    }

    #[private]
    pub fn on_withdraw(&mut self, account_id: AccountId, nonce: u64, amount: U128) -> bool {
        assert_eq!(
            env::promise_results_count(),
            1,
            "expected one transfer result"
        );
        let credit = self
            .credits
            .get_mut(&account_id)
            .expect("no credit account");
        if credit.nonce != nonce || credit.in_flight != amount || amount.0 == 0 {
            return false;
        }
        let success = env::promise_result_checked(0, 0).is_ok();
        self.in_flight = sub(self.in_flight, amount.0);
        if success {
            self.withdrawn = add(self.withdrawn, amount.0);
        } else {
            credit.available.0 = add(credit.available.0, amount.0);
            self.available = add(self.available, amount.0);
        }
        credit.in_flight = U128(0);
        event(
            "withdrawal_completed",
            &near_sdk::serde_json::json!({"account":account_id,"nonce":nonce,"amount":amount,"success":success}),
        );
        success
    }

    pub fn get_dataset(&self, seller: AccountId, name: String) -> Option<Dataset> {
        self.datasets.get(&(seller, name)).cloned()
    }
    pub fn get_job(&self, job_id: u64) -> Option<Job> {
        self.jobs.get(&job_id).cloned()
    }
    pub fn get_delivery(&self, job_id: u64, index: u8) -> Option<Delivery> {
        self.deliveries.get(&(job_id, index)).cloned()
    }
    pub fn get_credit(&self, account_id: AccountId) -> Option<Credit> {
        self.credits.get(&account_id).cloned()
    }
    pub fn get_job_count(&self) -> u64 {
        self.next_job
    }
    pub fn get_protocol(&self) -> near_sdk::serde_json::Value {
        near_sdk::serde_json::json!({"name":"wbts-data-escrow","version":"1.0.0","scheme":"inferpay-keyed-merkle-sha256-v1","entry_kind":"fact","max_keys":MAX_KEYS,"proof_depth":32,"evidence":"public","settlement":"eager-per-key","block_height":env::block_height()})
    }
    pub fn check_opening(&self, root: String, key: String, opening: Opening) -> bool {
        proof::verify(&root, &key, &opening).is_ok()
    }
    pub fn get_accounting(&self) -> Accounting {
        Accounting {
            deposits: U128(self.deposits),
            locked: U128(self.locked),
            available: U128(self.available),
            in_flight: U128(self.in_flight),
            withdrawn: U128(self.withdrawn),
            retained_storage_charges: U128(self.retained_storage_charges),
            conserved: self.deposits
                == add(
                    add(
                        add(self.locked, self.available),
                        add(self.in_flight, self.withdrawn),
                    ),
                    self.retained_storage_charges,
                ),
            contract_balance: U128(env::account_balance().as_yoctonear()),
            current_storage_bytes: env::storage_usage(),
        }
    }
}

impl DataEscrow {
    fn open(&self, id: u64) -> Job {
        let job = self.jobs.get(&id).expect("unknown job").clone();
        assert_eq!(job.close_reason, CloseReason::Open, "job is closed");
        job
    }
    fn ensure_account(&mut self, id: &AccountId) {
        self.credits.entry(id.clone()).or_default();
    }
    fn credit(&mut self, id: &AccountId, amount: u128) {
        let credit = self.credits.get_mut(id).expect("unfunded credit account");
        credit.available.0 = add(credit.available.0, amount);
        self.available = add(self.available, amount);
    }
    fn flush(&mut self) {
        self.datasets.flush();
        self.jobs.flush();
        self.deliveries.flush();
        self.credits.flush();
    }
    fn fund(&mut self, before: u64, principal: u128) {
        let caller = env::predecessor_account_id();
        self.ensure_account(&caller);
        self.flush();
        let bytes = env::storage_usage().saturating_sub(before);
        let storage = u128::from(bytes)
            .checked_mul(env::storage_byte_cost().as_yoctonear())
            .expect("storage cost overflow");
        let deposit = env::attached_deposit().as_yoctonear();
        assert!(
            deposit >= add(principal, storage),
            "insufficient price and storage deposit"
        );
        self.deposits = add(self.deposits, deposit);
        self.retained_storage_charges = add(self.retained_storage_charges, storage);
        let credit = self.credits.get_mut(&caller).unwrap();
        credit.retained_storage_paid.0 = add(credit.retained_storage_paid.0, storage);
        self.credit(&caller, deposit - principal - storage);
    }
    fn no_growth(&mut self, before: u64) {
        self.flush();
        assert!(env::storage_usage() <= before, "unfunded storage growth");
    }
    fn close(&mut self, mut job: Job, reason: CloseReason) -> Job {
        let before = env::storage_usage();
        self.locked = sub(self.locked, job.remaining.0);
        self.credit(&job.buyer, job.remaining.0);
        job.refunded.0 = add(job.refunded.0, job.remaining.0);
        job.remaining = U128(0);
        job.close_reason = reason;
        self.jobs.insert(job.id, job.clone());
        self.no_growth(before);
        event("job_closed", &job);
        job
    }
}

fn add(a: u128, b: u128) -> u128 {
    a.checked_add(b).expect("amount overflow")
}
fn sub(a: u128, b: u128) -> u128 {
    a.checked_sub(b).expect("amount underflow")
}
fn call_limit() {
    assert!(
        env::input().is_none_or(|bytes| bytes.len() <= MAX_CALL_BYTES),
        "call exceeds 8 KiB"
    );
}
fn event<T: near_sdk::serde::Serialize>(name: &str, data: &T) {
    env::log_str(&format!(
        "EVENT_JSON:{}",
        near_sdk::serde_json::json!({"standard":"wbts_data_escrow","version":"1.0.0","event":name,"data":[data]})
    ));
}
