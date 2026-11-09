//! Fresh-state v2: participant-funded direct edges, optimistic claims and recoverable credits.
//! No migration from the historical creator-funded ABI is provided.
// Flat JSON ABI methods intentionally expose individual transaction arguments.
#![allow(clippy::too_many_arguments)]
pub mod t2;
use near_sdk::{
    AccountId, Gas, NearToken, PanicOnDefault, Promise, PublicKey, env,
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
pub struct RunLog {
    creator: AccountId,
    verifier: VerifierPin,
    task_spec: Base64VecU8,
    agents: Vec<AccountId>,
    events: Vec<LogEvent>,
    head_hash: String,
}

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
#[derive(Clone, PartialEq, Eq)]
pub enum ClaimStatus {
    Pending,
    AdjudicatedUpheld,
    AdjudicatedOverturned,
    Finalized,
}

#[near(serializers = [json, borsh])]
#[derive(Clone, PartialEq, Eq)]
pub enum EconomicOutcome {
    UnchallengedAccepted,
    Upheld,
    Overturned,
}

#[near(serializers = [json, borsh])]
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct EdgeId {
    pub run_id: String,
    pub assignment_seq: u64,
}

#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct Edge {
    edge_id: EdgeId,
    delegator: AccountId,
    worker: AccountId,
    accept_deadline: u64,
    work_deadline: u64,
    parameters: Parameters,
    window: u64,
    delegator_reserved: U128,
    worker_reserved: U128,
    accepted: bool,
    completion_seq: Option<u64>,
    completion_height: u64,
    active_claim: Option<u64>,
    attempted_f1: bool,
    attempted_f3: bool,
    accepted_claim: Option<u64>,
    corrected_deadline: u64,
    corrected_class: Option<FailureClass>,
    released: bool,
}
impl Edge {
    fn cutoff(&self) -> u64 {
        add_height(
            if self.completion_seq.is_some() {
                self.completion_height
            } else {
                self.work_deadline
            },
            self.window,
        )
    }
    fn attempted(&self, class: &FailureClass) -> bool {
        match class {
            FailureClass::F1 => self.attempted_f1,
            FailureClass::F3 => self.attempted_f3,
            _ => true,
        }
    }
}

#[near(serializers = [json])]
pub struct EdgeView {
    #[serde(flatten)]
    edge: Edge,
    normal_claim_deadline: u64,
    release_after: u64,
}

#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct Claim {
    claim_id: u64,
    edge_id: EdgeId,
    reporter: AccountId,
    blamed: AccountId,
    failure_class: FailureClass,
    payloads: Vec<t2::Payload>,
    evidence_tx_hashes: Vec<String>,
    bond: U128,
    challenge_bond: U128,
    claim_height: u64,
    challenge_deadline: u64,
    status: ClaimStatus,
    challenger: Option<AccountId>,
    corrected_attribution: Option<AccountId>,
    evidence_snapshot: t2::EvidenceSnapshot,
    adjudication_result: Option<t2::Verification>,
    economic_outcome: Option<EconomicOutcome>,
    // Fixed slots let permissionless finalization grow optional fields without storage funding.
    padding: Vec<u8>,
}

#[near(serializers = [json, borsh])]
#[derive(Clone, Default)]
pub struct Credit {
    available: U128,
    in_flight: U128,
    nonce: u64,
    retained_storage_paid: U128,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct LogAnchor {
    owner: AccountId,
    parameters: Parameters,
    runs: LookupMap<String, RunLog>,
    edges: LookupMap<EdgeId, Edge>,
    claims: LookupMap<u64, Claim>,
    credits: LookupMap<AccountId, Credit>,
    reputations: LookupMap<AccountId, u8>,
    challenge_bonus_consumed: LookupMap<(String, AccountId), bool>,
    next_claim_id: u64,
    challenge_window_blocks: u64,
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
            runs: LookupMap::new(b"r2".as_slice()),
            edges: LookupMap::new(b"e2".as_slice()),
            claims: LookupMap::new(b"c2".as_slice()),
            credits: LookupMap::new(b"w2".as_slice()),
            reputations: LookupMap::new(b"p2".as_slice()),
            challenge_bonus_consumed: LookupMap::new(b"b2".as_slice()),
            next_claim_id: 0,
            challenge_window_blocks: if demo.unwrap_or(false) { 60 } else { 600 },
            treasury_balance: U128(0),
        }
    }
    #[payable]
    pub fn register(
        &mut self,
        run_id: String,
        agents: Vec<AccountId>,
        verifier: VerifierPin,
        task_spec: Base64VecU8,
    ) {
        call_limit();
        assert!(
            !run_id.is_empty() && run_id.len() <= 128,
            "run ID must have 1..128 bytes"
        );
        assert!(!self.runs.contains_key(&run_id), "run already registered");
        assert!(
            !agents.is_empty() && agents.len() <= 100,
            "agents must have 1..100 entries"
        );
        assert_eq!(
            agents.iter().collect::<BTreeSet<_>>().len(),
            agents.len(),
            "agents must be unique"
        );
        assert!(is_hash(&verifier.task_spec_hash), "invalid spec hash");
        assert_eq!(
            verifier.verifier_hash,
            t2::implementation_hash(),
            "unsupported verifier implementation"
        );
        assert_eq!(
            hex::encode(env::sha256(&task_spec.0)),
            verifier.task_spec_hash,
            "task spec hash mismatch"
        );
        t2::parse_spec(&task_spec.0).unwrap_or_else(|e| env::panic_str(e));
        let before = env::storage_usage();
        self.runs.insert(
            run_id,
            RunLog {
                creator: env::predecessor_account_id(),
                verifier,
                task_spec,
                agents,
                events: vec![],
                head_hash: String::new(),
            },
        );
        self.fund(before, 0);
    }
    #[payable]
    pub fn offer_assignment(
        &mut self,
        run_id: String,
        worker: AccountId,
        payload_hash: String,
        accept_deadline: u64,
        work_deadline: u64,
        seq: u64,
        prev_hash: String,
    ) -> EdgeId {
        call_limit();
        assert!(
            env::block_height() < accept_deadline && accept_deadline < work_deadline,
            "invalid assignment deadlines"
        );
        add_height(work_deadline, self.challenge_window_blocks);
        let before = env::storage_usage();
        let delegator = env::predecessor_account_id();
        let run = self.runs.get(&run_id).expect("unknown run");
        assert!(
            run.agents.contains(&worker),
            "worker must be registered agent"
        );
        let id = EdgeId {
            run_id: run_id.clone(),
            assignment_seq: seq,
        };
        self.append(
            &run_id,
            seq,
            delegator.clone(),
            "task_assigned".into(),
            payload_hash,
            prev_hash,
            Some(t2::TaskLink {
                assignment_seq: seq,
                worker: worker.clone(),
                parent_assignment_seq: None,
            }),
        );
        let stake = self.parameters.stake.0;
        self.ensure_account(&delegator);
        self.edges.insert(
            id.clone(),
            Edge {
                edge_id: id.clone(),
                delegator,
                worker,
                accept_deadline,
                work_deadline,
                parameters: self.parameters.clone(),
                window: self.challenge_window_blocks,
                delegator_reserved: U128(stake),
                worker_reserved: U128(0),
                accepted: false,
                completion_seq: None,
                completion_height: 0,
                active_claim: None,
                attempted_f1: false,
                attempted_f3: false,
                accepted_claim: None,
                corrected_deadline: 0,
                corrected_class: None,
                released: false,
            },
        );
        self.fund(before, stake);
        id
    }
    #[payable]
    pub fn accept_assignment(&mut self, edge_id: EdgeId) {
        call_limit();
        let before = env::storage_usage();
        let mut edge = self.edge(&edge_id);
        assert_eq!(
            edge.worker,
            env::predecessor_account_id(),
            "only worker may accept"
        );
        assert!(!edge.accepted && !edge.released, "offer not open");
        assert!(
            env::block_height() <= edge.accept_deadline,
            "acceptance deadline expired"
        );
        self.ensure_account(&edge.worker);
        edge.accepted = true;
        edge.worker_reserved = edge.parameters.stake;
        let stake = edge.worker_reserved.0;
        self.edges.insert(edge_id, edge);
        self.fund(before, stake);
    }
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
        call_limit();
        assert_eq!(actor, env::predecessor_account_id(), "actor must be caller");
        assert!(kind != "task_assigned", "use offer_assignment");
        let before = env::storage_usage();
        if kind == "settlement" {
            let link = task.as_ref().expect("completion requires task binding");
            assert!(
                link.parent_assignment_seq.is_none(),
                "nested delegation unsupported"
            );
            let id = EdgeId {
                run_id: run_id.clone(),
                assignment_seq: link.assignment_seq,
            };
            let mut edge = self.edge(&id);
            assert!(
                edge.accepted && !edge.released && edge.accepted_claim.is_none(),
                "edge not active"
            );
            assert_eq!(actor, edge.worker, "only worker may complete");
            assert_eq!(link.worker, edge.worker, "worker binding mismatch");
            assert!(
                edge.completion_seq.is_none(),
                "assignment already completed"
            );
            assert!(
                env::block_height() <= edge.work_deadline,
                "work deadline expired"
            );
            edge.completion_seq = Some(seq);
            edge.completion_height = env::block_height();
            self.edges.insert(id, edge);
        } else {
            assert!(task.is_none(), "unsupported task event kind");
        }
        let head = self.append(&run_id, seq, actor, kind, payload_hash, prev_hash, task);
        self.fund(before, 0);
        head
    }
    #[payable]
    pub fn file_claim(
        &mut self,
        edge_id: EdgeId,
        failure_class: FailureClass,
        payloads: Vec<t2::Payload>,
        evidence_tx_hashes: Vec<String>,
    ) -> u64 {
        call_limit();
        let mut edge = self.edge(&edge_id);
        assert!(
            edge.accepted && !edge.released && edge.accepted_claim.is_none(),
            "edge not claimable"
        );
        assert!(edge.active_claim.is_none(), "edge has active claim");
        assert!(
            matches!(failure_class, FailureClass::F1 | FailureClass::F3),
            "unsupported claim class"
        );
        assert!(
            !edge.attempted(&failure_class),
            "allegation already attempted"
        );
        assert!(
            env::block_height() <= edge.cutoff()
                || (edge.corrected_class.as_ref() == Some(&failure_class)
                    && env::block_height() <= edge.corrected_deadline),
            "claim window expired"
        );
        if failure_class == FailureClass::F1 {
            assert!(edge.completion_seq.is_some(), "F1 requires completion");
        }
        let run = self.runs.get(&edge_id.run_id).expect("unknown run");
        let seqs: Vec<u64> = std::iter::once(edge_id.assignment_seq)
            .chain(edge.completion_seq)
            .collect();
        assert_eq!(payloads.len(), seqs.len(), "incomplete task evidence");
        assert_eq!(
            evidence_tx_hashes.len(),
            seqs.len(),
            "evidence reference count mismatch"
        );
        let mut total = run.task_spec.0.len();
        for ((p, seq), tx) in payloads.iter().zip(&seqs).zip(&evidence_tx_hashes) {
            assert_eq!(p.seq, *seq, "evidence sequence mismatch");
            assert!(
                p.bytes.0.len() <= t2::MAX_PAYLOAD_BYTES,
                "payload exceeds byte limit"
            );
            total = total
                .checked_add(p.bytes.0.len())
                .expect("evidence overflow");
            assert_eq!(
                hex::encode(env::sha256(&p.bytes.0)),
                run.events[*seq as usize].payload_hash,
                "payload hash mismatch"
            );
            assert!(
                tx.len() <= 44
                    && near_sdk::bs58::decode(tx)
                        .into_vec()
                        .is_ok_and(|h| h.len() == 32),
                "invalid transaction reference"
            );
        }
        assert!(
            total <= t2::MAX_EVIDENCE_BYTES,
            "evidence exceeds byte limit"
        );
        let snapshot = t2::EvidenceSnapshot {
            event_count: run.events.len() as u64,
            head_hash: run.head_hash.clone(),
            challenge_height: env::block_height(),
        };
        let before = env::storage_usage();
        let reporter = env::predecessor_account_id();
        self.ensure_account(&reporter);
        let id = self.next_claim_id;
        self.next_claim_id = id.checked_add(1).expect("claim ID overflow");
        let bond = edge.parameters.blame_bond;
        let blamed = if failure_class == FailureClass::F1 {
            edge.worker.clone()
        } else {
            edge.delegator.clone()
        };
        if failure_class == FailureClass::F1 {
            edge.attempted_f1 = true;
        } else {
            edge.attempted_f3 = true;
        }
        edge.active_claim = Some(id);
        self.claims.insert(
            id,
            Claim {
                claim_id: id,
                edge_id: edge_id.clone(),
                reporter,
                blamed,
                failure_class,
                payloads,
                evidence_tx_hashes,
                bond,
                challenge_bond: U128(0),
                claim_height: env::block_height(),
                challenge_deadline: add_height(env::block_height(), edge.window),
                status: ClaimStatus::Pending,
                challenger: None,
                corrected_attribution: None,
                evidence_snapshot: snapshot,
                adjudication_result: None,
                economic_outcome: None,
                padding: vec![0; 1],
            },
        );
        self.edges.insert(edge_id, edge);
        self.fund(before, bond.0);
        id
    }
    pub fn preview_adjudication(&self, claim_id: u64) -> t2::Verification {
        let claim = self.claims.get(&claim_id).expect("unknown claim");
        let edge = self.edge(&claim.edge_id);
        let run = self.runs.get(&claim.edge_id.run_id).expect("unknown run");
        let spec = t2::parse_spec(&run.task_spec.0).unwrap_or_else(|e| env::panic_str(e));
        let input = &claim.payloads[0].bytes.0;
        let output = claim.payloads.get(1).map(|p| p.bytes.0.as_slice());
        let check = t2::verify(&spec, input, output).unwrap_or_else(|e| env::panic_str(e));
        let finding = match check {
            t2::Check::PreconditionFault => Some(t2::Finding {
                agent: edge.delegator,
                failure_class: FailureClass::F3,
                event_seq: claim.edge_id.assignment_seq,
                origin: t2::Origin::Introduction,
            }),
            t2::Check::PostconditionFault => Some(t2::Finding {
                agent: edge.worker,
                failure_class: FailureClass::F1,
                event_seq: claim.payloads[1].seq,
                origin: t2::Origin::Introduction,
            }),
            t2::Check::Passed | t2::Check::Incomplete => None,
        };
        let upheld = finding
            .as_ref()
            .is_some_and(|f| f.agent == claim.blamed && f.failure_class == claim.failure_class);
        let origin = finding.as_ref().and_then(|f| {
            if f.failure_class == claim.failure_class {
                Some(t2::Origin::Introduction)
            } else if f.failure_class == FailureClass::F3 && claim.failure_class == FailureClass::F1
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
            assignment_seq: claim.edge_id.assignment_seq,
            snapshot: claim.evidence_snapshot.clone(),
        }
    }
    #[payable]
    pub fn challenge(&mut self, claim_id: u64) -> t2::Verification {
        call_limit();
        let mut claim = self.claims.get(&claim_id).cloned().expect("unknown claim");
        assert!(claim.status == ClaimStatus::Pending, "claim not pending");
        assert!(
            env::block_height() <= claim.challenge_deadline,
            "challenge window expired"
        );
        let result = self.preview_adjudication(claim_id);
        let before = env::storage_usage();
        let challenger = env::predecessor_account_id();
        self.ensure_account(&challenger);
        self.challenge_bonus_consumed
            .entry((claim.edge_id.run_id.clone(), challenger.clone()))
            .or_insert(false);
        claim.challenger = Some(challenger);
        claim.challenge_bond = self.edge(&claim.edge_id).parameters.challenge_bond;
        let principal = claim.challenge_bond.0;
        claim.corrected_attribution = result.finding.as_ref().map(|f| f.agent.clone());
        claim.status = match result.verdict {
            t2::Verdict::Upheld => ClaimStatus::AdjudicatedUpheld,
            t2::Verdict::Overturned => ClaimStatus::AdjudicatedOverturned,
        };
        claim.adjudication_result = Some(result.clone());
        self.claims.insert(claim_id, claim);
        self.fund(before, principal);
        result
    }
    pub fn finalize(&mut self, claim_id: u64) {
        let mut claim = self.claims.get(&claim_id).cloned().expect("unknown claim");
        if claim.status == ClaimStatus::Finalized {
            return;
        }
        assert!(
            env::block_height() > claim.challenge_deadline,
            "challenge window still open"
        );
        let before = env::storage_usage();
        let mut edge = self.edge(&claim.edge_id);
        assert_eq!(edge.active_claim, Some(claim_id), "active claim mismatch");
        let old_size = near_sdk::borsh::to_vec(&edge).unwrap().len();
        let (recipient, principal, slashed, outcome) =
            if claim.status == ClaimStatus::AdjudicatedOverturned {
                let challenger = claim.challenger.clone().expect("missing challenger");
                self.rep_sub(&claim.reporter, 15);
                let key = (claim.edge_id.run_id.clone(), challenger.clone());
                if !*self
                    .challenge_bonus_consumed
                    .get(&key)
                    .expect("missing bonus slot")
                {
                    self.challenge_bonus_consumed.insert(key, true);
                    self.rep_add(&challenger, 5);
                }
                self.rep_add(&claim.blamed, 2);
                if let Some(f) = claim
                    .adjudication_result
                    .as_ref()
                    .and_then(|r| r.finding.as_ref())
                    && f.failure_class != claim.failure_class
                    && !edge.attempted(&f.failure_class)
                {
                    edge.corrected_class = Some(f.failure_class.clone());
                    edge.corrected_deadline = add_height(env::block_height(), edge.window);
                }
                (
                    challenger,
                    claim.challenge_bond.0,
                    claim.bond.0,
                    EconomicOutcome::Overturned,
                )
            } else {
                let reserved = if claim.failure_class == FailureClass::F1 {
                    &mut edge.worker_reserved
                } else {
                    &mut edge.delegator_reserved
                };
                let slash = claim.bond.0.min(reserved.0);
                reserved.0 -= slash;
                self.rep_sub(&claim.blamed, 10);
                edge.accepted_claim = Some(claim_id);
                let challenged = claim.status == ClaimStatus::AdjudicatedUpheld;
                (
                    claim.reporter.clone(),
                    claim.bond.0,
                    add(slash, claim.challenge_bond.0),
                    if challenged {
                        EconomicOutcome::Upheld
                    } else {
                        EconomicOutcome::UnchallengedAccepted
                    },
                )
            };
        let reward = (slashed / 5) * 4 + ((slashed % 5) * 4) / 5;
        self.credit(&recipient, add(principal, reward));
        self.treasury_balance.0 = add(self.treasury_balance.0, slashed - reward);
        edge.active_claim = None;
        // Optional active_claim releases eight bytes; accepted_claim/corrected_class fit that space.
        assert!(
            near_sdk::borsh::to_vec(&edge).unwrap().len() <= old_size,
            "unfunded edge growth"
        );
        self.edges.insert(claim.edge_id.clone(), edge);
        let old_claim_size = near_sdk::borsh::to_vec(&claim).unwrap().len();
        claim.economic_outcome = Some(outcome);
        claim.padding.clear();
        claim.status = ClaimStatus::Finalized;
        claim.bond = U128(0);
        claim.challenge_bond = U128(0);
        assert!(
            near_sdk::borsh::to_vec(&claim).unwrap().len() <= old_claim_size,
            "unfunded claim growth"
        );
        self.claims.insert(claim_id, claim);
        self.flush();
        assert!(env::storage_usage() <= before, "unfunded finalize storage");
    }
    pub fn release_edge(&mut self, edge_id: EdgeId) {
        let mut edge = self.edge(&edge_id);
        if edge.released {
            return;
        }
        assert!(edge.active_claim.is_none(), "edge has active claim");
        if !edge.accepted {
            assert!(
                env::block_height() > edge.accept_deadline,
                "offer still open"
            );
        } else if edge.accepted_claim.is_none() {
            assert!(
                env::block_height() > edge.cutoff().max(edge.corrected_deadline),
                "claim window still open"
            );
        }
        let before = env::storage_usage();
        self.credit(&edge.delegator, edge.delegator_reserved.0);
        if edge.accepted {
            self.credit(&edge.worker, edge.worker_reserved.0);
        }
        edge.delegator_reserved = U128(0);
        edge.worker_reserved = U128(0);
        edge.released = true;
        self.edges.insert(edge_id, edge);
        self.flush();
        assert!(env::storage_usage() <= before, "unfunded release storage");
    }
    #[payable]
    pub fn withdraw(&mut self, amount: U128) -> Promise {
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
        let c = self.credits.get_mut(&caller).expect("no credit account");
        assert_eq!(c.in_flight.0, 0, "withdrawal already pending");
        c.available.0 = add(c.available.0, 1);
        assert!(
            amount.0 > 0 && amount.0 <= c.available.0,
            "invalid withdrawal amount"
        );
        c.available.0 -= amount.0;
        c.in_flight = amount;
        c.nonce = c.nonce.checked_add(1).expect("nonce overflow");
        let nonce = c.nonce;
        Promise::new(caller.clone())
            .transfer(NearToken::from_yoctonear(amount.0))
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(Gas::from_tgas(15))
                    .on_withdraw(caller, nonce, amount),
            )
    }
    #[private]
    pub fn on_withdraw(&mut self, account_id: AccountId, nonce: u64, amount: U128) -> bool {
        assert_eq!(
            env::promise_results_count(),
            1,
            "expected one transfer result"
        );
        let c = self
            .credits
            .get_mut(&account_id)
            .expect("no credit account");
        if c.nonce != nonce || c.in_flight != amount || amount.0 == 0 {
            return false;
        }
        let success = env::promise_result_checked(0, 0).is_ok();
        if !success {
            c.available.0 = add(c.available.0, amount.0);
        }
        c.in_flight = U128(0);
        success
    }
    pub fn get_parameters(&self) -> Parameters {
        self.parameters.clone()
    }
    pub fn update_parameters(&mut self, stake: U128, blame_bond: U128, challenge_bond: U128) {
        assert_eq!(env::predecessor_account_id(), self.owner, "owner only");
        assert!(
            stake.0 >= NearToken::from_millinear(250).as_yoctonear()
                && blame_bond.0 > 0
                && blame_bond == challenge_bond,
            "invalid economic parameters"
        );
        self.parameters = Parameters {
            stake,
            blame_bond,
            challenge_bond,
        };
    }
    pub fn get_edge(&self, edge_id: EdgeId) -> Option<EdgeView> {
        let edge = self.edges.get(&edge_id)?.clone();
        let normal_claim_deadline = edge.cutoff();
        let release_after = if !edge.accepted {
            edge.accept_deadline
        } else if edge.accepted_claim.is_some() {
            0
        } else {
            normal_claim_deadline.max(edge.corrected_deadline)
        };
        Some(EdgeView {
            edge,
            normal_claim_deadline,
            release_after,
        })
    }
    pub fn get_credit(&self, account_id: AccountId) -> Option<Credit> {
        self.credits.get(&account_id).cloned()
    }
    pub fn get_claim(&self, claim_id: u64) -> Option<Claim> {
        self.claims.get(&claim_id).cloned()
    }
    pub fn get_run(&self, run_id: String) -> Option<RunLog> {
        self.runs.get(&run_id).cloned()
    }
    pub fn get_reputation(&self, agent: AccountId) -> u8 {
        *self.reputations.get(&agent).unwrap_or(&100)
    }
    pub fn get_treasury(&self) -> U128 {
        self.treasury_balance
    }
    pub fn get_verifier_info(&self) -> t2::VerifierInfo {
        t2::info()
    }
    pub fn get_event(&self, run_id: String, seq: u64) -> Option<LogEvent> {
        self.runs
            .get(&run_id)?
            .events
            .get(usize::try_from(seq).ok()?)
            .cloned()
    }
    pub fn run_len(&self, run_id: String) -> u64 {
        self.runs.get(&run_id).map_or(0, |r| r.events.len() as u64)
    }
    pub fn verify_chain(&self, run_id: String) -> Option<u64> {
        let run = self.runs.get(&run_id).expect("unknown run");
        let mut head = String::new();
        for (i, e) in run.events.iter().enumerate() {
            if e.seq != i as u64 || e.prev_hash != head {
                return Some(i as u64);
            }
            head = event_hash(e);
        }
        if head != run.head_hash {
            Some(run.events.len() as u64)
        } else {
            None
        }
    }
}
impl LogAnchor {
    fn edge(&self, id: &EdgeId) -> Edge {
        self.edges.get(id).cloned().expect("unknown edge")
    }
    fn ensure_account(&mut self, id: &AccountId) {
        self.credits.entry(id.clone()).or_default();
        self.reputations.entry(id.clone()).or_insert(100);
    }
    fn credit(&mut self, id: &AccountId, amount: u128) {
        let c = self.credits.get_mut(id).expect("unfunded credit account");
        c.available.0 = add(c.available.0, amount);
    }
    fn rep_sub(&mut self, id: &AccountId, amount: u8) {
        let r = self.reputations.get_mut(id).expect("unfunded reputation");
        *r = r.saturating_sub(amount);
    }
    fn rep_add(&mut self, id: &AccountId, amount: u8) {
        let r = self.reputations.get_mut(id).expect("unfunded reputation");
        *r = r.saturating_add(amount).min(120);
    }
    fn flush(&mut self) {
        self.runs.flush();
        self.edges.flush();
        self.claims.flush();
        self.credits.flush();
        self.reputations.flush();
        self.challenge_bonus_consumed.flush();
    }
    fn fund(&mut self, before: u64, principal: u128) {
        let caller = env::predecessor_account_id();
        self.ensure_account(&caller);
        self.flush();
        let storage = u128::from(
            env::storage_usage()
                .checked_sub(before)
                .expect("unexpected storage shrink"),
        ) * env::storage_byte_cost().as_yoctonear();
        let required = add(principal, storage);
        let deposit = env::attached_deposit().as_yoctonear();
        assert!(
            deposit >= required,
            "insufficient principal and storage deposit"
        );
        let c = self.credits.get_mut(&caller).unwrap();
        c.available.0 = add(c.available.0, deposit - required);
        c.retained_storage_paid.0 = add(c.retained_storage_paid.0, storage);
    }
    #[allow(clippy::too_many_arguments)]
    fn append(
        &mut self,
        run_id: &String,
        seq: u64,
        actor: AccountId,
        kind: String,
        payload_hash: String,
        prev_hash: String,
        task: Option<t2::TaskLink>,
    ) -> String {
        assert!(is_hash(&payload_hash), "invalid payload hash");
        assert!(!kind.is_empty() && kind.len() <= 64, "invalid event kind");
        let run = self.runs.get_mut(run_id).expect("unknown run");
        assert!(
            run.agents.contains(&actor),
            "actor must be registered agent"
        );
        assert_eq!(seq, run.events.len() as u64, "seq must extend the log");
        assert!(run.events.len() < t2::MAX_EVENTS, "event limit reached");
        assert_eq!(prev_hash, run.head_hash, "prev_hash must equal head");
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
        run.events.push(event);
        run.head_hash = head.clone();
        head
    }
}
fn add(a: u128, b: u128) -> u128 {
    a.checked_add(b).expect("amount overflow")
}
fn add_height(a: u64, b: u64) -> u64 {
    a.checked_add(b).expect("height overflow")
}
fn call_limit() {
    assert!(
        env::input().is_none_or(|i| i.len() <= t2::MAX_CALL_BYTES),
        "call exceeds byte limit"
    );
}
fn event_hash(event: &LogEvent) -> String {
    hex::encode(env::sha256(
        near_sdk::borsh::to_vec(event).expect("serialize event"),
    ))
}

fn is_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
