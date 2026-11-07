//! Gate-1 registered run logs with creator-funded participation stakes.
//! Unchallenged F1 claims can finalize after their window. Challenge handling,
//! adjudication and participation-stake release are not implemented in this slice.

use near_sdk::{
    AccountId, NearToken, PanicOnDefault, Promise, env, json_types::U128, near, store::LookupMap,
};
use std::collections::BTreeSet;

#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct LogEvent {
    pub seq: u64,
    pub actor: AccountId,
    pub kind: String,
    pub payload_hash: String,
    pub prev_hash: String,
    pub block_height: u64,
}

#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct AgentStake {
    account_id: AccountId,
    locked_stake: U128,
}

#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct RunLog {
    creator: AccountId,
    agents: Vec<AgentStake>,
    events: Vec<LogEvent>,
    head_hash: String,
}

/// The Gate-1 slice admits only F1. Other classes arrive with their verifiers.
#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub enum FailureClass {
    F1,
}

#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub enum ClaimStatus {
    Pending,
    Finalized,
}

#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct Claim {
    claim_id: u64,
    run_id: String,
    reporter: AccountId,
    blamed: AccountId,
    failure_class: FailureClass,
    evidence_seqs: Vec<u64>,
    evidence_payload_hashes: Vec<String>,
    evidence_tx_hashes: Vec<String>,
    bond: U128,
    claim_height: u64,
    challenge_deadline: u64,
    status: ClaimStatus,
    challenger: Option<AccountId>,
    corrected_attribution: Option<AccountId>,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct LogAnchor {
    runs: LookupMap<String, RunLog>,
    claims: LookupMap<u64, Claim>,
    next_claim_id: u64,
    challenge_window_blocks: u64,
    reputations: LookupMap<AccountId, u8>,
    treasury_balance: U128,
}

#[near]
impl LogAnchor {
    #[init]
    pub fn new(demo: Option<bool>) -> Self {
        Self {
            runs: LookupMap::new(b"r"),
            claims: LookupMap::new(b"c"),
            next_claim_id: 0,
            challenge_window_blocks: if demo.unwrap_or(false) { 60 } else { 600 },
            reputations: LookupMap::new(b"p"),
            treasury_balance: U128(0),
        }
    }

    /// Register unique agents and fund 1 NEAR of participation stake for each.
    /// The run creator supplies all stakes plus measured registration storage.
    /// Only excess deposit is refunded here; stake release is a later method.
    /// Task/verifier pinning is deferred to its dedicated work item (2.4).
    #[payable]
    pub fn register(&mut self, run_id: String, agents: Vec<AccountId>) {
        assert!(!run_id.is_empty(), "run_id must not be empty");
        assert!(!self.runs.contains_key(&run_id), "run already registered");
        assert!(!agents.is_empty(), "agents must not be empty");
        let unique: BTreeSet<_> = agents.iter().collect();
        assert_eq!(unique.len(), agents.len(), "agents must be unique");
        let stake = NearToken::from_near(1).as_yoctonear();
        let total_stake = stake
            .checked_mul(agents.len() as u128)
            .expect("stake overflow");
        let storage_before = env::storage_usage();
        // Initialize once, at registration, so finalization updates an existing
        // byte instead of creating unfunded reputation storage. Re-registration
        // in another run must not reset a previously reduced score.
        for account in &agents {
            if !self.reputations.contains_key(account) {
                self.reputations.insert(account.clone(), 100);
            }
        }
        self.reputations.flush();
        self.runs.insert(
            run_id,
            RunLog {
                creator: env::predecessor_account_id(),
                agents: agents
                    .into_iter()
                    .map(|account_id| AgentStake {
                        account_id,
                        locked_stake: U128(stake),
                    })
                    .collect(),
                events: Vec::new(),
                head_hash: String::new(),
            },
        );
        self.runs.flush();
        let storage_cost = u128::from(env::storage_usage() - storage_before)
            * env::storage_byte_cost().as_yoctonear();
        let required = total_stake
            .checked_add(storage_cost)
            .expect("deposit overflow");
        let deposit = env::attached_deposit().as_yoctonear();
        assert!(
            deposit >= required,
            "insufficient stake and storage deposit"
        );
        if deposit > required {
            Promise::new(env::predecessor_account_id())
                .transfer(NearToken::from_yoctonear(deposit - required))
                .detach();
        }
    }

    /// Append at the current head, paying for the measured storage growth.
    /// Hashes are lowercase SHA-256 hex; the genesis predecessor is empty.
    /// Returns SHA-256(Borsh(event)), including its inclusion block height.
    #[payable]
    pub fn anchor(
        &mut self,
        run_id: String,
        seq: u64,
        actor: AccountId,
        kind: String,
        payload_hash: String,
        prev_hash: String,
    ) -> String {
        assert!(!run_id.is_empty(), "run_id must not be empty");
        assert!(!kind.is_empty(), "kind must not be empty");
        assert_eq!(actor, env::predecessor_account_id(), "actor must be caller");
        assert!(
            is_hash(&payload_hash),
            "payload_hash must be lowercase SHA-256 hex"
        );
        let storage_before = env::storage_usage();
        let log = self.runs.get(&run_id).expect("unknown run");
        assert!(
            log.agents.iter().any(|agent| agent.account_id == actor),
            "actor must be registered agent"
        );
        assert_eq!(seq, log.events.len() as u64, "seq must extend the log");
        assert_eq!(prev_hash, log.head_hash, "prev_hash must equal head");

        let event = LogEvent {
            seq,
            actor,
            kind,
            payload_hash,
            prev_hash,
            block_height: env::block_height(),
        };
        let head = event_hash(&event);
        let log = self.runs.get_mut(&run_id).expect("unknown run");
        log.events.push(event);
        log.head_hash = head.clone();
        // LookupMap buffers writes. Flush before measuring trie storage growth.
        self.runs.flush();
        let storage_cost = u128::from(env::storage_usage() - storage_before)
            * env::storage_byte_cost().as_yoctonear();
        let deposit = env::attached_deposit().as_yoctonear();
        assert!(deposit >= storage_cost, "insufficient storage deposit");
        if deposit > storage_cost {
            let _refund = Promise::new(env::predecessor_account_id())
                .transfer(NearToken::from_yoctonear(deposit - storage_cost));
        }
        head
    }

    /// Return the first inconsistent link/sequence index, or the event count if
    /// the final head differs. This is a consistency check over stored events;
    /// payload bytes must be checked off-chain against their committed hashes.
    /// An unknown run is an error, rather than an intact chain.
    pub fn verify_chain(&self, run_id: String) -> Option<u64> {
        let log = self.runs.get(&run_id).expect("unknown run");
        let mut head = String::new();
        for (index, event) in log.events.iter().enumerate() {
            if event.seq != index as u64 || event.prev_hash != head {
                return Some(index as u64);
            }
            head = event_hash(event);
        }
        (head != log.head_hash).then_some(log.events.len() as u64)
    }

    pub fn get_event(&self, run_id: String, seq: u64) -> Option<LogEvent> {
        let index = usize::try_from(seq).ok()?;
        self.runs.get(&run_id)?.events.get(index).cloned()
    }

    pub fn run_len(&self, run_id: String) -> u64 {
        self.runs
            .get(&run_id)
            .map_or(0, |log| log.events.len() as u64)
    }

    pub fn get_run(&self, run_id: String) -> Option<RunLog> {
        self.runs.get(&run_id).cloned()
    }

    /// File a pending F1 claim with a 0.25 NEAR bond plus storage deposit.
    /// Evidence hashes are lowercase SHA-256 hex, matching the anchor ABI.
    /// Historical transaction linkage and blame correctness are not verified
    /// here: transaction hashes are references, checked only for base58 format.
    #[payable]
    pub fn file_claim(
        &mut self,
        run_id: String,
        blamed: AccountId,
        failure_class: FailureClass,
        evidence_seqs: Vec<u64>,
        evidence_payload_hashes: Vec<String>,
        evidence_tx_hashes: Vec<String>,
    ) -> u64 {
        let log = self.runs.get(&run_id).expect("unknown run");
        assert!(
            log.agents.iter().any(|agent| agent.account_id == blamed),
            "blamed must be registered agent"
        );
        assert!(!evidence_seqs.is_empty(), "evidence must not be empty");
        assert_eq!(
            evidence_seqs.len(),
            evidence_payload_hashes.len(),
            "evidence lengths must match"
        );
        assert_eq!(
            evidence_seqs.len(),
            evidence_tx_hashes.len(),
            "evidence lengths must match"
        );
        assert!(
            evidence_seqs.windows(2).all(|pair| pair[0] < pair[1]),
            "evidence sequences must be unique and increasing"
        );
        for ((seq, payload_hash), tx_hash) in evidence_seqs
            .iter()
            .zip(&evidence_payload_hashes)
            .zip(&evidence_tx_hashes)
        {
            let index = usize::try_from(*seq).expect("evidence sequence out of range");
            let event = log
                .events
                .get(index)
                .expect("evidence event does not exist");
            assert_eq!(
                payload_hash, &event.payload_hash,
                "evidence payload hash mismatch"
            );
            assert!(
                tx_hash.len() <= 44
                    && near_sdk::bs58::decode(tx_hash)
                        .into_vec()
                        .is_ok_and(|hash| hash.len() == 32),
                "invalid evidence transaction hash"
            );
        }
        let bond = NearToken::from_millinear(250).as_yoctonear();
        let claim_id = self.next_claim_id;
        let next_claim_id = claim_id.checked_add(1).expect("claim ID overflow");
        let claim_height = env::block_height();
        let challenge_deadline = claim_height
            .checked_add(self.challenge_window_blocks)
            .expect("challenge deadline overflow");
        let storage_before = env::storage_usage();
        self.claims.insert(
            claim_id,
            Claim {
                claim_id,
                run_id,
                reporter: env::predecessor_account_id(),
                blamed,
                failure_class,
                evidence_seqs,
                evidence_payload_hashes,
                evidence_tx_hashes,
                bond: U128(bond),
                claim_height,
                challenge_deadline,
                status: ClaimStatus::Pending,
                challenger: None,
                corrected_attribution: None,
            },
        );
        self.claims.flush();
        let storage_cost = u128::from(env::storage_usage() - storage_before)
            * env::storage_byte_cost().as_yoctonear();
        let required = bond.checked_add(storage_cost).expect("deposit overflow");
        let deposit = env::attached_deposit().as_yoctonear();
        assert!(deposit >= required, "insufficient bond and storage deposit");
        self.next_claim_id = next_claim_id;
        if deposit > required {
            Promise::new(env::predecessor_account_id())
                .transfer(NearToken::from_yoctonear(deposit - required))
                .detach();
        }
        claim_id
    }

    pub fn get_claim(&self, claim_id: u64) -> Option<Claim> {
        self.claims.get(&claim_id).cloned()
    }

    /// Accept an unchallenged claim once height > its recorded deadline.
    /// This optimistic acceptance does not establish that the allegation is true.
    /// Repeated finalization is a no-op. Remaining participation stakes stay locked.
    pub fn finalize(&mut self, claim_id: u64) {
        let mut claim = self.claims.get(&claim_id).cloned().expect("unknown claim");
        if matches!(claim.status, ClaimStatus::Finalized) {
            return;
        }
        assert!(claim.challenger.is_none(), "claim must be unchallenged");
        assert!(
            env::block_height() > claim.challenge_deadline,
            "challenge window still open"
        );
        let slash = NearToken::from_millinear(250).as_yoctonear();
        let reward = slash * 4 / 5;
        let treasury_share = slash - reward;
        let payout = claim.bond.0.checked_add(reward).expect("payout overflow");
        let reputation = self
            .reputations
            .get(&claim.blamed)
            .copied()
            .expect("missing registered reputation");
        let log = self.runs.get_mut(&claim.run_id).expect("unknown run");
        let agent = log
            .agents
            .iter_mut()
            .find(|agent| agent.account_id == claim.blamed)
            .expect("blamed must be registered agent");
        agent.locked_stake.0 = agent
            .locked_stake
            .0
            .checked_sub(slash)
            .expect("insufficient locked stake");
        self.treasury_balance.0 = self
            .treasury_balance
            .0
            .checked_add(treasury_share)
            .expect("treasury overflow");
        self.reputations
            .insert(claim.blamed.clone(), reputation.saturating_sub(10));
        claim.status = ClaimStatus::Finalized;
        claim.bond = U128(0);
        let reporter = claim.reporter.clone();
        self.claims.insert(claim_id, claim);
        // All storage fields updated here have fixed serialized sizes. Claim
        // storage and reputation creation were funded at filing/registration.
        self.runs.flush();
        self.claims.flush();
        self.reputations.flush();
        // Propagate the transfer outcome to the caller. State changes and the
        // later transfer are separate receipts; the live proof must check both.
        Promise::new(reporter)
            .transfer(NearToken::from_yoctonear(payout))
            .as_return()
            .detach();
    }

    pub fn get_reputation(&self, agent: AccountId) -> u8 {
        self.reputations.get(&agent).copied().unwrap_or(100)
    }

    /// Accumulated allocation retained by the contract; no spending rule in v1.
    pub fn get_treasury(&self) -> U128 {
        self.treasury_balance
    }
}

fn event_hash(event: &LogEvent) -> String {
    hex::encode(env::sha256(
        &near_sdk::borsh::to_vec(event).expect("serialize event"),
    ))
}

fn is_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
