//! Gate-1 registered run logs with creator-funded participation stakes.
//! Challenged direct T2 tasks support persisted causal adjudication. Challenged
//! payouts and releasing participation stakes are later work items.

pub mod t2;

use near_sdk::{
    AccountId, NearToken, PanicOnDefault, Promise, PublicKey, env,
    json_types::{Base64VecU8, U128},
    near,
    store::LookupMap,
};
use std::collections::BTreeSet;

#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct Parameters {
    stake: U128,
    blame_bond: U128,
    challenge_bond: U128,
}

/// Identity pins supplied by the creator; hashes do not verify execution.
#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct VerifierPin {
    task_spec_hash: String,
    verifier_hash: String,
    verifier_key: Option<PublicKey>,
}

#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct LogEvent {
    pub seq: u64,
    pub actor: AccountId,
    pub kind: String,
    pub payload_hash: String,
    pub prev_hash: String,
    pub block_height: u64,
    pub task: Option<t2::TaskLink>,
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
    verifier: VerifierPin,
    agents: Vec<AgentStake>,
    events: Vec<LogEvent>,
    head_hash: String,
}

/// Checkable failure classes; verifier dispatch is implemented separately.
#[near(serializers = [json, borsh])]
#[derive(Clone, PartialEq, Eq)]
pub enum FailureClass {
    F1,
    F2,
    F3,
    F4,
    F5a,
}

#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub enum ClaimStatus {
    Pending,
    Challenged,
    Finalized,
    AdjudicatedUpheld,
    AdjudicatedOverturned,
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
    /// U8: a newly named agent needs its own bonded claim and defense window before any reputation penalty.
    corrected_attribution: Option<AccountId>,
    challenge_bond: U128,
    verifier_hash: String,
    assignment_seq: Option<u64>,
    evidence_snapshot: Option<t2::EvidenceSnapshot>,
    adjudication_result: Option<t2::Verification>,
    /// Challenger-funded physical storage, consumed by the persisted result.
    adjudication_padding: Vec<u8>,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct LogAnchor {
    owner: AccountId,
    parameters: Parameters,
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
            owner: env::predecessor_account_id(),
            parameters: Parameters {
                stake: U128(NearToken::from_near(1).as_yoctonear()),
                blame_bond: U128(NearToken::from_millinear(250).as_yoctonear()),
                challenge_bond: U128(NearToken::from_millinear(250).as_yoctonear()),
            },
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
    /// The embedded T2 mode requires the pinned spec bytes for admission.
    #[payable]
    pub fn register(
        &mut self,
        run_id: String,
        agents: Vec<AccountId>,
        verifier: VerifierPin,
        task_spec: Option<Base64VecU8>,
    ) {
        assert!(
            is_hash(&verifier.task_spec_hash) && is_hash(&verifier.verifier_hash),
            "invalid verifier pin"
        );
        assert_eq!(
            verifier.verifier_hash,
            t2::implementation_hash(),
            "unsupported verifier implementation"
        );
        {
            let spec = task_spec
                .as_ref()
                .expect("embedded verifier requires task spec");
            assert_eq!(
                hex::encode(env::sha256(&spec.0)),
                verifier.task_spec_hash,
                "task spec hash mismatch"
            );
            t2::parse_spec(&spec.0).unwrap_or_else(|error| env::panic_str(error));
        }
        assert!(!run_id.is_empty(), "run_id must not be empty");
        assert!(!self.runs.contains_key(&run_id), "run already registered");
        assert!(!agents.is_empty(), "agents must not be empty");
        let unique: BTreeSet<_> = agents.iter().collect();
        assert_eq!(unique.len(), agents.len(), "agents must be unique");
        let stake = self.parameters.stake.0;
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
                verifier,
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
        task: Option<t2::TaskLink>,
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
        if log.verifier.verifier_hash == t2::implementation_hash() {
            assert!(
                log.events.len() < t2::MAX_EVENTS,
                "embedded verifier event limit reached"
            );
            match kind.as_str() {
                "task_assigned" => {
                    let link = task.as_ref().expect("assignment requires task binding");
                    assert_eq!(
                        link.assignment_seq, seq,
                        "assignment must bind its own sequence"
                    );
                    assert!(
                        link.parent_assignment_seq.is_none(),
                        "nested T2 delegation is unsupported"
                    );
                    assert!(
                        log.agents
                            .iter()
                            .any(|agent| agent.account_id == link.worker),
                        "worker must be registered agent"
                    );
                }
                "settlement" => {
                    let link = task.as_ref().expect("completion requires task binding");
                    let index = usize::try_from(link.assignment_seq)
                        .expect("assignment sequence out of range");
                    let assigned = log.events.get(index).expect("unknown assignment");
                    assert_eq!(assigned.kind, "task_assigned", "not an assignment");
                    assert!(
                        assigned.task.as_ref() == Some(link),
                        "completion task binding mismatch"
                    );
                    assert_eq!(actor, link.worker, "only assigned worker may complete");
                    assert!(
                        !log.events
                            .iter()
                            .any(|event| event.kind == "settlement"
                                && event.task.as_ref() == Some(link)),
                        "assignment already completed"
                    );
                }
                _ => assert!(task.is_none(), "unsupported task event kind"),
            }
        } else {
            assert!(task.is_none(), "task binding requires supported verifier");
        }

        let event = LogEvent {
            seq,
            actor,
            kind,
            payload_hash,
            prev_hash,
            block_height: env::block_height(),
            task,
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
            Promise::new(env::predecessor_account_id())
                .transfer(NearToken::from_yoctonear(deposit - storage_cost))
                .detach();
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

    /// File a pending claim with the current bond plus storage deposit.
    /// Evidence hashes are lowercase SHA-256 hex, matching the anchor ABI.
    /// Historical transaction linkage and blame correctness are not verified
    /// here: transaction hashes are references, checked only for base58 format.
    #[payable]
    pub fn file_claim(
        &mut self,
        run_id: String,
        blamed: AccountId,
        failure_class: FailureClass,
        verifier_hash: String,
        evidence_seqs: Vec<u64>,
        evidence_payload_hashes: Vec<String>,
        evidence_tx_hashes: Vec<String>,
    ) -> u64 {
        let log = self.runs.get(&run_id).expect("unknown run");
        assert_eq!(
            verifier_hash, log.verifier.verifier_hash,
            "verifier hash mismatch"
        );
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
        let assignment_seq = if verifier_hash == t2::implementation_hash() {
            assert!(
                matches!(failure_class, FailureClass::F1 | FailureClass::F3),
                "unsupported T2 claim class"
            );
            let seq = evidence_seqs[0];
            let assignment =
                &log.events[usize::try_from(seq).expect("assignment sequence out of range")];
            assert_eq!(
                assignment.kind, "task_assigned",
                "first evidence must identify assignment"
            );
            let link = assignment
                .task
                .as_ref()
                .expect("assignment has no task binding");
            for cited in &evidence_seqs {
                if let Some(other) = &log.events[*cited as usize].task {
                    assert!(other == link, "evidence refers to another task");
                }
            }
            Some(seq)
        } else {
            None
        };
        let bond = self.parameters.blame_bond.0;
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
                challenge_bond: U128(0),
                verifier_hash,
                assignment_seq,
                evidence_snapshot: None,
                adjudication_result: None,
                adjudication_padding: Vec::new(),
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

    pub fn get_parameters(&self) -> Parameters {
        self.parameters.clone()
    }

    pub fn update_parameters(&mut self, stake: U128, blame_bond: U128, challenge_bond: U128) {
        assert_eq!(env::predecessor_account_id(), self.owner, "owner only");
        assert!(
            stake.0 >= NearToken::from_millinear(250).as_yoctonear(),
            "stake below slash"
        );
        assert!(
            blame_bond.0 > 0 && blame_bond.0 == challenge_bond.0,
            "bonds must be positive and matching"
        );
        self.parameters = Parameters {
            stake,
            blame_bond,
            challenge_bond,
        };
    }

    /// First eligible caller locks the dispute and funds snapshot/result storage
    /// separately from the matching bond. No payload bytes are supplied here.
    #[payable]
    pub fn challenge(&mut self, claim_id: u64) {
        let mut claim = self.claims.get(&claim_id).cloned().expect("unknown claim");
        assert!(
            matches!(claim.status, ClaimStatus::Pending),
            "claim must be pending"
        );
        assert!(
            env::block_height() <= claim.challenge_deadline,
            "challenge window expired"
        );
        let storage_before = env::storage_usage();
        let bond = claim.bond.0;
        claim.challenger = Some(env::predecessor_account_id());
        claim.challenge_bond = U128(bond);
        claim.status = ClaimStatus::Challenged;
        let log = self.runs.get(&claim.run_id).expect("unknown run");
        claim.evidence_snapshot = Some(t2::EvidenceSnapshot {
            event_count: log.events.len() as u64,
            head_hash: log.head_hash.clone(),
            challenge_height: env::block_height(),
        });
        claim.adjudication_padding =
            vec![0; adjudication_capacity(claim.evidence_snapshot.as_ref().unwrap())];
        self.claims.insert(claim_id, claim);
        self.claims.flush();
        let cost = u128::from(env::storage_usage() - storage_before)
            * env::storage_byte_cost().as_yoctonear();
        let required = bond.checked_add(cost).expect("deposit overflow");
        let deposit = env::attached_deposit().as_yoctonear();
        assert!(
            deposit >= required,
            "insufficient challenge bond and storage deposit"
        );
        if deposit > required {
            Promise::new(env::predecessor_account_id())
                .transfer(NearToken::from_yoctonear(deposit - required))
                .detach();
        }
    }

    pub fn get_claim(&self, claim_id: u64) -> Option<Claim> {
        self.claims.get(&claim_id).cloned()
    }

    pub fn get_verifier_info(&self) -> t2::VerifierInfo {
        t2::info()
    }

    /// Persist the deterministic preview once. Anyone may pay gas; no principal,
    /// reputation or transfers change. Replay returns the original full result.
    pub fn adjudicate(
        &mut self,
        claim_id: u64,
        task_spec: Base64VecU8,
        payloads: Vec<t2::Payload>,
    ) -> t2::Verification {
        let mut claim = self.claims.get(&claim_id).cloned().expect("unknown claim");
        if let Some(result) = &claim.adjudication_result {
            return result.clone();
        }
        let result = self.preview_adjudication(claim_id, task_spec, payloads);
        let before_len = near_sdk::borsh::to_vec(&claim)
            .expect("serialize claim")
            .len();
        claim.status = match result.verdict {
            t2::Verdict::Upheld => ClaimStatus::AdjudicatedUpheld,
            t2::Verdict::Overturned => ClaimStatus::AdjudicatedOverturned,
        };
        claim.corrected_attribution = result.finding.as_ref().map(|fault| fault.agent.clone());
        claim.adjudication_result = Some(result.clone());
        claim.adjudication_padding.clear();
        let used = near_sdk::borsh::to_vec(&claim)
            .expect("serialize claim")
            .len();
        claim.adjudication_padding.resize(
            before_len
                .checked_sub(used)
                .expect("adjudication result exceeds funded slot"),
            0,
        );
        assert_eq!(
            near_sdk::borsh::to_vec(&claim)
                .expect("serialize claim")
                .len(),
            before_len
        );
        let storage_before = env::storage_usage();
        self.claims.insert(claim_id, claim);
        self.claims.flush();
        assert_eq!(
            env::storage_usage(),
            storage_before,
            "adjudication storage changed"
        );
        result
    }

    /// Compute-only 2.8 entry point. No claim state, money or reputation changes.
    /// adjudicate reuses this exact computation before persisting its result.
    pub fn preview_adjudication(
        &self,
        claim_id: u64,
        task_spec: Base64VecU8,
        payloads: Vec<t2::Payload>,
    ) -> t2::Verification {
        assert!(
            env::input().is_none_or(|input| input.len() <= t2::MAX_CALL_BYTES),
            "adjudication call exceeds byte limit"
        );
        assert!(
            payloads.len() <= 2,
            "direct T2 task requires at most two payloads"
        );
        let mut bytes = task_spec.0.len();
        for payload in &payloads {
            assert!(
                payload.bytes.0.len() <= t2::MAX_PAYLOAD_BYTES,
                "payload exceeds byte limit"
            );
            bytes = bytes
                .checked_add(payload.bytes.0.len())
                .expect("evidence size overflow");
        }
        assert!(
            bytes <= t2::MAX_EVIDENCE_BYTES,
            "evidence exceeds byte limit"
        );
        let claim = self.claims.get(&claim_id).expect("unknown claim");
        assert!(
            matches!(claim.status, ClaimStatus::Challenged),
            "claim must be challenged"
        );
        assert_eq!(
            claim.verifier_hash,
            t2::implementation_hash(),
            "unsupported verifier implementation"
        );
        assert!(
            matches!(claim.failure_class, FailureClass::F1 | FailureClass::F3),
            "unsupported T2 claim class"
        );
        let log = self.runs.get(&claim.run_id).expect("unknown run");
        assert_eq!(
            claim.verifier_hash, log.verifier.verifier_hash,
            "verifier pin mismatch"
        );
        assert_eq!(
            hex::encode(env::sha256(&task_spec.0)),
            log.verifier.task_spec_hash,
            "task spec hash mismatch"
        );
        let spec = t2::parse_spec(&task_spec.0).unwrap_or_else(|error| env::panic_str(error));
        let snapshot = claim
            .evidence_snapshot
            .as_ref()
            .expect("missing evidence snapshot");
        let count = usize::try_from(snapshot.event_count).expect("snapshot out of range");
        assert!(
            count > 0 && count <= t2::MAX_EVENTS && count <= log.events.len(),
            "unsupported evidence snapshot"
        );
        let events = &log.events[..count];
        let mut head = String::new();
        for (seq, event) in events.iter().enumerate() {
            assert!(
                event.seq == seq as u64 && event.prev_hash == head,
                "invalid stored event chain"
            );
            head = event_hash(event);
        }
        assert_eq!(head, snapshot.head_hash, "snapshot head mismatch");
        let seq = claim
            .assignment_seq
            .expect("claim does not bind an assignment");
        let assignment = events
            .get(usize::try_from(seq).expect("assignment out of range"))
            .expect("assignment outside snapshot");
        assert_eq!(assignment.kind, "task_assigned", "not an assignment");
        let link = assignment.task.as_ref().expect("missing task binding");
        assert!(
            link.assignment_seq == seq && link.parent_assignment_seq.is_none(),
            "unsupported task binding"
        );
        let mut completions = events
            .iter()
            .filter(|event| event.kind == "settlement" && event.task.as_ref() == Some(link));
        let completion = completions.next();
        assert!(completions.next().is_none(), "ambiguous task completions");
        if let Some(event) = completion {
            assert_eq!(event.actor, link.worker, "completion actor mismatch");
        }
        assert_eq!(
            payloads.len(),
            if completion.is_some() { 2 } else { 1 },
            "incomplete task evidence"
        );
        assert!(
            payloads.windows(2).all(|pair| pair[0].seq != pair[1].seq),
            "duplicate payload sequence"
        );
        for payload in &payloads {
            assert!(
                payload.seq == seq || completion.is_some_and(|event| payload.seq == event.seq),
                "payload refers to another task or snapshot"
            );
            let event =
                &events[usize::try_from(payload.seq).expect("payload sequence out of range")];
            assert_eq!(
                hex::encode(env::sha256(&payload.bytes.0)),
                event.payload_hash,
                "payload hash mismatch"
            );
        }
        let input = payloads
            .iter()
            .find(|payload| payload.seq == seq)
            .expect("missing assignment payload");
        let output = completion.map(|event| {
            payloads
                .iter()
                .find(|payload| payload.seq == event.seq)
                .expect("missing completion payload")
                .bytes
                .0
                .as_slice()
        });
        let check =
            t2::verify(&spec, &input.bytes.0, output).unwrap_or_else(|error| env::panic_str(error));
        let finding = match check {
            t2::Check::PreconditionFault => Some(t2::Finding {
                agent: assignment.actor.clone(),
                failure_class: FailureClass::F3,
                event_seq: seq,
                origin: t2::Origin::Introduction,
            }),
            t2::Check::PostconditionFault => Some(t2::Finding {
                agent: link.worker.clone(),
                failure_class: FailureClass::F1,
                event_seq: completion.expect("missing completion").seq,
                origin: t2::Origin::Introduction,
            }),
            t2::Check::Passed => None,
        };
        let upheld = finding.as_ref().is_some_and(|fault| {
            fault.agent == claim.blamed && fault.failure_class == claim.failure_class
        });
        let origin = finding.as_ref().and_then(|fault| {
            if fault.failure_class == claim.failure_class {
                Some(t2::Origin::Introduction)
            } else if fault.failure_class == FailureClass::F3
                && claim.failure_class == FailureClass::F1
            {
                Some(t2::Origin::Propagation)
            } else {
                None
            }
        });
        t2::Verification {
            verdict: if upheld {
                t2::Verdict::Upheld
            } else {
                t2::Verdict::Overturned
            },
            finding,
            origin,
            assignment_seq: seq,
            snapshot: snapshot.clone(),
        }
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

/// Derive the maximum growth from the actual Borsh schema: both optional fields
/// become Some, with the largest AccountId (64 bytes), finding and origin. Enum
/// variants have equal encoded widths. Snapshot bytes are fixed at challenge.
fn adjudication_capacity(snapshot: &t2::EvidenceSnapshot) -> usize {
    let agent: AccountId = "a".repeat(64).parse().expect("maximum account id");
    let result = t2::Verification {
        verdict: t2::Verdict::Upheld,
        finding: Some(t2::Finding {
            agent: agent.clone(),
            failure_class: FailureClass::F1,
            event_seq: u64::MAX,
            origin: t2::Origin::Introduction,
        }),
        origin: Some(t2::Origin::Introduction),
        assignment_seq: u64::MAX,
        snapshot: snapshot.clone(),
    };
    // Each Option already has its one-byte tag when None; only its content grows.
    near_sdk::borsh::to_vec(&result)
        .expect("serialize result capacity")
        .len()
        + near_sdk::borsh::to_vec(&agent)
            .expect("serialize account capacity")
            .len()
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
