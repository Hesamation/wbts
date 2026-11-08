use base64::{Engine, engine::general_purpose::STANDARD};
use near_contract_sim::{Balance, Delta, Gas, Sim};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::OnceLock};

const CONTRACT: &str = "engine.near";
const COORD: &str = "coord.near";
const WORKER: &str = "worker.near";
const REVIEWER: &str = "reviewer.near";
const CALLER: &str = "caller.near";

fn digest(bytes: &[u8]) -> String {
    near_contract_sim::nearcore::near_primitives::hash::hash(bytes)
        .0
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn wasm() -> &'static Vec<u8> {
    static WASM: OnceLock<Vec<u8>> = OnceLock::new();
    WASM.get_or_init(|| {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../contract/Cargo.toml")
            .canonicalize()
            .unwrap();
        let path = cargo_near_build::build_with_cli(
            cargo_near_build::BuildOpts::builder()
                .manifest_path(manifest.to_str().unwrap())
                .build(),
        )
        .expect("build current T2 source");
        println!("Fresh verifier WASM: {path}");
        std::fs::read(path).unwrap()
    })
}

struct Run {
    sim: Sim,
    spec: Vec<u8>,
    verifier: String,
    bodies: BTreeMap<u64, Vec<u8>>,
    transactions: BTreeMap<u64, String>,
    head: String,
}

impl Run {
    fn new(bps: u16) -> Self {
        Self::with_spec(
            serde_json::to_vec(&json!({"task_type": "t2_fee_split_v1", "fee_basis_points": bps}))
                .unwrap(),
        )
    }

    fn with_spec(spec: Vec<u8>) -> Self {
        Self::with_worker(spec, WORKER)
    }

    fn with_worker(spec: Vec<u8>, worker: &str) -> Self {
        let mut sim = Sim::new();
        for name in ["sponsor.near", COORD, WORKER, REVIEWER, CALLER] {
            sim.create_account(name, Balance::from_near(100));
        }
        if worker != WORKER {
            sim.create_account(worker, Balance::from_near(100));
        }
        sim.deploy(CONTRACT, wasm().clone());
        // Sponsor/creator deliberately differs from the actual input producer.
        sim.call("sponsor.near", CONTRACT, "new")
            .args_json(json!({"demo": true}))
            .transact()
            .assert_success();
        let info: Value = sim.view(CONTRACT, "get_verifier_info", json!({})).json();
        let verifier = info["verifier_hash"].as_str().unwrap().to_owned();
        assert_eq!(
            info["identity_kind"],
            "source-and-dependency-manifest-sha256-v1"
        );
        sim.call("sponsor.near", CONTRACT, "register")
            .args_json(json!({"run_id": "t2", "agents": [COORD, worker, REVIEWER],
                "verifier": {"task_spec_hash": digest(&spec), "verifier_hash": verifier, "verifier_key": null},
                "task_spec": STANDARD.encode(&spec)}))
            .deposit(Balance::from_millinear(3050)).transact().assert_success();
        Self {
            sim,
            spec,
            verifier,
            bodies: BTreeMap::new(),
            transactions: BTreeMap::new(),
            head: String::new(),
        }
    }

    fn append(&mut self, actor: &str, kind: &str, body: &[u8], task: Option<Value>) -> u64 {
        let seq = self.bodies.len() as u64;
        let out = self
            .sim
            .call(actor, CONTRACT, "anchor")
            .args_json(
                json!({"run_id": "t2", "seq": seq, "actor": actor, "kind": kind,
                "payload_hash": digest(body), "prev_hash": self.head, "task": task}),
            )
            .deposit(Balance::from_millinear(20))
            .transact();
        out.assert_success();
        self.head = out.json();
        self.bodies.insert(seq, body.to_vec());
        self.transactions.insert(seq, out.hash.to_string());
        seq
    }

    fn assign(&mut self, worker: &str, body: &[u8]) -> u64 {
        self.append(COORD, "task_assigned", body, Some(json!({
            "assignment_seq": self.bodies.len(), "worker": worker, "parent_assignment_seq": null,
        })))
    }

    fn complete(&mut self, assignment: u64, worker: &str, body: &[u8]) -> u64 {
        self.append(
            worker,
            "settlement",
            body,
            Some(json!({
                "assignment_seq": assignment, "worker": worker, "parent_assignment_seq": null,
            })),
        )
    }

    fn file(&mut self, assignment: u64, blamed: &str, class: &str) -> u64 {
        self.file_as(COORD, assignment, blamed, class)
    }

    fn file_as(&mut self, reporter: &str, assignment: u64, blamed: &str, class: &str) -> u64 {
        let out = self
            .sim
            .call(reporter, CONTRACT, "file_claim")
            .args_json(
                json!({"run_id": "t2", "blamed": blamed, "failure_class": class,
                "verifier_hash": self.verifier, "evidence_seqs": [assignment],
                "evidence_payload_hashes": [digest(&self.bodies[&assignment])],
                "evidence_tx_hashes": [self.transactions[&assignment]]}),
            )
            .deposit(Balance::from_millinear(280))
            .transact();
        out.assert_success();
        out.json()
    }

    fn challenge(&mut self, id: u64) {
        self.sim
            .call(WORKER, CONTRACT, "challenge")
            .args_json(json!({"claim_id": id}))
            .deposit(Balance::from_millinear(270))
            .transact()
            .assert_success();
    }

    fn args(&self, id: u64, seqs: &[u64]) -> Value {
        json!({"claim_id": id, "task_spec": STANDARD.encode(&self.spec),
            "payloads": seqs.iter().map(|seq| json!({"seq": seq, "bytes": STANDARD.encode(&self.bodies[seq])})).collect::<Vec<_>>()})
    }

    fn preview(&self, id: u64, seqs: &[u64]) -> Value {
        self.sim
            .view(CONTRACT, "preview_adjudication", self.args(id, seqs))
            .json()
    }

    fn state(&self, id: u64) -> Value {
        json!({"claim": self.sim.view(CONTRACT, "get_claim", json!({"claim_id": id})).json::<Value>(),
            "run": self.sim.view(CONTRACT, "get_run", json!({"run_id": "t2"})).json::<Value>(),
            "worker_rep": self.sim.view(CONTRACT, "get_reputation", json!({"agent": WORKER})).json::<u8>(),
            "coordinator_rep": self.sim.view(CONTRACT, "get_reputation", json!({"agent": COORD})).json::<u8>(),
            "treasury": self.sim.view(CONTRACT, "get_treasury", json!({})).json::<Value>()})
    }
}

fn input() -> Vec<u8> {
    br#"{"entries":["10000","25000","15000"],"declared_total":"50000"}"#.to_vec()
}
fn passing() -> &'static [u8] {
    br#"{"fee":"1250","net":"48750"}"#
}

#[test]
fn causal_verdicts_authenticate_agents_and_do_not_move_money() {
    for (body, output, class, blamed, expected, fault, agent, origin) in [
        (
            input(),
            passing(),
            "F1",
            WORKER,
            "Overturned",
            None,
            None,
            None,
        ),
        (
            input(),
            b"wrong".as_slice(),
            "F1",
            WORKER,
            "Upheld",
            Some("F1"),
            Some(WORKER),
            Some("Introduction"),
        ),
        (
            br#"{"entries":["50000"],"declared_total":"49999"}"#.to_vec(),
            b"wrong".as_slice(),
            "F1",
            WORKER,
            "Overturned",
            Some("F3"),
            Some(COORD),
            Some("Propagation"),
        ),
        (
            b"{bad-input".to_vec(),
            passing(),
            "F3",
            COORD,
            "Upheld",
            Some("F3"),
            Some(COORD),
            Some("Introduction"),
        ),
        (
            input(),
            b"wrong".as_slice(),
            "F1",
            REVIEWER,
            "Overturned",
            Some("F1"),
            Some(WORKER),
            Some("Introduction"),
        ),
    ] {
        let mut r = Run::new(250);
        let root = r.assign(WORKER, &body);
        let done = r.complete(root, WORKER, output);
        let id = r.file(root, blamed, class);
        r.challenge(id);
        let before = r.state(id);
        let args = r.args(id, &[root, done]);
        let caller_before = r.sim.balance(CALLER).as_yoctonear();
        let out = r
            .sim
            .call(CALLER, CONTRACT, "preview_adjudication")
            .args_json(args)
            .gas(Gas::from_teragas(100))
            .transact();
        out.assert_success();
        let value: Value = out.json();
        assert_eq!(value["verdict"], expected);
        assert_eq!(
            value["origin"],
            origin.map_or(Value::Null, |label| json!(label))
        );
        if let Some(fault) = fault {
            assert_eq!(value["finding"]["failure_class"], fault);
            assert_eq!(value["finding"]["agent"], agent.unwrap());
            assert_eq!(value["finding"]["origin"], "Introduction");
        } else {
            assert_eq!(value["finding"], Value::Null);
        }
        assert_eq!(r.preview(id, &[done, root]), value); // order cannot steer verdict
        assert_eq!(r.state(id), before); // no conviction/payout/reputation mutation
        assert!(out.effects().near.is_empty());
        assert_eq!(out.effects().storage_bytes(CONTRACT), 0);
        assert_eq!(
            caller_before - r.sim.balance(CALLER).as_yoctonear(),
            out.effects().gas_paid().as_yoctonear()
        );
        r.sim.findings().assert_none();
    }
}

#[test]
fn reject_forged_incomplete_or_cross_task_evidence_without_verdict() {
    let mut r = Run::new(250);
    // Earlier sibling F3 must not override the actual claim's worker F1.
    let sibling = r.assign(REVIEWER, b"invalid unrelated input");
    let sibling_done = r.complete(sibling, REVIEWER, b"invalid unrelated result");
    let root = r.assign(WORKER, &input());
    let done = r.complete(root, WORKER, b"wrong result");
    let id = r.file(root, WORKER, "F1");
    r.sim
        .view(CONTRACT, "preview_adjudication", r.args(id, &[root, done]))
        .assert_panic_contains("claim must be challenged");
    r.challenge(id);
    assert_eq!(
        r.preview(id, &[root, done])["finding"]["failure_class"],
        "F1"
    );
    let before = r.state(id);
    let original = r.args(id, &[root, done]);
    let mut forged = original.clone();
    forged["payloads"][0]["bytes"] = json!(STANDARD.encode(b"attacker bytes"));
    let mut wrong_spec = original.clone();
    wrong_spec["task_spec"] = json!(STANDARD.encode(b"{}"));
    for (args, message) in [
        (forged, "payload hash mismatch"),
        (wrong_spec, "task spec hash mismatch"),
        (r.args(id, &[root]), "incomplete task evidence"),
        (r.args(id, &[root, root]), "duplicate payload sequence"),
        (
            r.args(id, &[sibling, sibling_done]),
            "payload refers to another task or snapshot",
        ),
    ] {
        for method in ["preview_adjudication", "adjudicate"] {
            let out = r
                .sim
                .call(CALLER, CONTRACT, method)
                .args_json(args.clone())
                .gas(Gas::from_teragas(100))
                .transact();
            out.assert_panic_contains(message);
            assert!(out.effects().near.is_empty());
            assert_eq!(out.effects().storage_bytes(CONTRACT), 0);
            assert_eq!(r.state(id), before);
        }
    }
    let expected = r.preview(id, &[root, done]);
    assert_eq!(r.persist(id, &[root, done]), expected); // rejection is retryable
    r.sim.findings().assert_none();
}

#[test]
fn challenge_snapshot_includes_prior_defense_and_excludes_later_appends() {
    let mut r = Run::new(250);
    let root = r.assign(WORKER, &input());
    let id = r.file(root, WORKER, "F1");
    // Defense committed after filing, before challenge, is eligible.
    let done = r.complete(root, WORKER, passing());
    r.challenge(id);
    let expected = r.preview(id, &[root, done]);
    assert_eq!(expected["verdict"], "Overturned");
    assert_eq!(expected["snapshot"]["event_count"], 2);
    r.append(
        REVIEWER,
        "note",
        b"new evidence cannot change a frozen snapshot",
        None,
    );
    assert_eq!(r.preview(id, &[root, done]), expected);
    assert_eq!(r.persist(id, &[root, done]), expected);
    // A completion committed after a different task's challenge is excluded.
    let second_root = r.assign(WORKER, &input());
    let second_id = r.file(second_root, WORKER, "F1");
    r.challenge(second_id);
    let late = r.complete(second_root, WORKER, passing());
    r.sim
        .view(
            CONTRACT,
            "preview_adjudication",
            r.args(second_id, &[second_root, late]),
        )
        .assert_panic_contains("incomplete task evidence");
    r.sim
        .view(
            CONTRACT,
            "preview_adjudication",
            r.args(second_id, &[second_root]),
        )
        .assert_panic_contains("missing completion");
    let before = r.state(second_id);
    let args = r.args(second_id, &[second_root, late]);
    r.sim
        .call(CALLER, CONTRACT, "adjudicate")
        .args_json(args)
        .transact()
        .assert_panic_contains("incomplete task evidence");
    assert_eq!(r.state(second_id), before);
    r.sim.findings().assert_none();
}

#[test]
fn unknown_verifier_registration_rejects_without_funding() {
    let mut r = Run::new(250);
    for unknown in ["ff".repeat(32), {
        let mut typo = r.verifier.clone();
        typo.replace_range(0..1, if &typo[..1] == "0" { "1" } else { "0" });
        typo
    }] {
        let rejected = r.sim.call("sponsor.near", CONTRACT, "register")
            .args_json(json!({"run_id": "unknown", "agents": [COORD, WORKER],
                "verifier": {"task_spec_hash": digest(&r.spec), "verifier_hash": unknown, "verifier_key": null},
                "task_spec": STANDARD.encode(&r.spec)}))
            .deposit(Balance::from_millinear(2050)).transact();
        rejected.assert_panic_contains("unsupported verifier implementation");
        assert_eq!(rejected.effects().near_net(CONTRACT), Delta::ZERO);
        assert_eq!(
            r.sim
                .view(CONTRACT, "get_run", json!({"run_id": "unknown"}))
                .json::<Value>(),
            Value::Null
        );
    }
    r.sim.call("sponsor.near", CONTRACT, "register")
        .args_json(json!({"run_id": "unknown", "agents": [COORD, WORKER],
            "verifier": {"task_spec_hash": digest(&r.spec), "verifier_hash": r.verifier, "verifier_key": null},
            "task_spec": STANDARD.encode(&r.spec)}))
        .deposit(Balance::from_millinear(2050)).transact().assert_success();
    r.sim.findings().assert_none();
}

#[test]
fn integer_exactness_matches_independent_python_bigint_reference() {
    let vectors: Value =
        serde_json::from_str(include_str!("../fixtures/t2_integer_vectors.json")).unwrap();
    for vector in vectors.as_array().unwrap() {
        let mut r = Run::new(vector["fee_basis_points"].as_u64().unwrap() as u16);
        let raw = serde_json::to_vec(
            &json!({"entries": [vector["total"]], "declared_total": vector["total"]}),
        )
        .unwrap();
        let root = r.assign(WORKER, &raw);
        let done = r.complete(root, WORKER, vector["output"].as_str().unwrap().as_bytes());
        let id = r.file(root, WORKER, "F1");
        r.challenge(id);
        let result = r.preview(id, &[root, done]);
        assert_eq!(result["verdict"], "Overturned", "{vector}");
        assert_eq!(result["finding"], Value::Null);
        r.sim.findings().assert_none();
    }
    for raw in [
        br#"{"entries":["340282366920938463463374607431768211455","1"],"declared_total":"0"}"#
            .as_slice(),
        br#"{"entries":["01"],"declared_total":"1"}"#.as_slice(),
        br#"{"entries":["1"],"declared_total":"1","extra":true}"#.as_slice(),
        br#"{"entries":["1"],"entries":["2"],"declared_total":"2"}"#.as_slice(),
    ] {
        let mut r = Run::new(250);
        let root = r.assign(WORKER, raw);
        let done = r.complete(root, WORKER, passing());
        let id = r.file(root, WORKER, "F1");
        r.challenge(id);
        assert_eq!(
            r.preview(id, &[root, done])["finding"]["failure_class"],
            "F3"
        );
        r.sim.findings().assert_none();
    }
    for entries in [Vec::<String>::new(), vec!["1".to_string(); 129]] {
        let mut r = Run::new(250);
        let raw =
            serde_json::to_vec(&json!({"entries": entries, "declared_total": "129"})).unwrap();
        let root = r.assign(WORKER, &raw);
        let done = r.complete(root, WORKER, passing());
        let id = r.file(root, WORKER, "F1");
        r.challenge(id);
        assert_eq!(
            r.preview(id, &[root, done])["finding"]["failure_class"],
            "F3"
        );
        r.sim.findings().assert_none();
    }
    for output in [
        br#"{"net":"48750","fee":"1250"}"#.as_slice(),
        b"{\"fee\":\"1250\",\"net\":\"48750\"}\n".as_slice(),
    ] {
        let mut r = Run::new(250);
        let root = r.assign(WORKER, &input());
        let done = r.complete(root, WORKER, output);
        let id = r.file(root, WORKER, "F1");
        r.challenge(id);
        assert_eq!(r.preview(id, &[root, done])["verdict"], "Upheld");
        r.sim.findings().assert_none();
    }
}

#[test]
fn task_admission_and_bindings_reject_ambiguous_authority() {
    let mut r = Run::new(250);
    for (spec, message) in [
        (
            br#"{"task_type":"t3","fee_basis_points":250}"#.as_slice(),
            "unsupported task spec",
        ),
        (
            br#"{"task_type":"t2_fee_split_v1","fee_basis_points":10001}"#.as_slice(),
            "unsupported task spec",
        ),
        (
            br#"{"task_type":"t2_fee_split_v1","fee_basis_points":250,"fee_basis_points":1}"#
                .as_slice(),
            "invalid task spec",
        ),
    ] {
        let out = r.sim.call("sponsor.near", CONTRACT, "register")
            .args_json(json!({"run_id": "invalid", "agents": [WORKER],
                "verifier": {"task_spec_hash": digest(spec), "verifier_hash": r.verifier, "verifier_key": null},
                "task_spec": STANDARD.encode(spec)}))
            .deposit(Balance::from_millinear(1050)).transact();
        out.assert_panic_contains(message);
        assert_eq!(out.effects().near_net(CONTRACT), Delta::ZERO);
    }
    let root = r.assign(WORKER, &input());
    let before = r.state_value();
    for (actor, task, message) in [
        (
            REVIEWER,
            json!({"assignment_seq": root, "worker": WORKER, "parent_assignment_seq": null}),
            "actor must be caller",
        ),
        (
            REVIEWER,
            json!({"assignment_seq": root, "worker": REVIEWER, "parent_assignment_seq": null}),
            "completion task binding mismatch",
        ),
        (
            REVIEWER,
            json!({"assignment_seq": root, "worker": WORKER, "parent_assignment_seq": null}),
            "only assigned worker may complete",
        ),
    ] {
        let declared_actor = if message == "actor must be caller" {
            WORKER
        } else {
            actor
        };
        let out = r.sim.call(actor, CONTRACT, "anchor")
            .args_json(json!({"run_id": "t2", "seq": 1, "actor": declared_actor, "kind": "settlement", "payload_hash": digest(passing()), "prev_hash": r.head, "task": task}))
            .deposit(Balance::from_millinear(20)).transact();
        out.assert_panic_contains(message);
    }
    assert_eq!(r.state_value(), before);
    let out = r.sim.call(COORD, CONTRACT, "anchor")
        .args_json(json!({"run_id": "t2", "seq": 1, "actor": COORD, "kind": "task_assigned", "payload_hash": digest(&input()), "prev_hash": r.head,
            "task": {"assignment_seq": 1, "worker": REVIEWER, "parent_assignment_seq": root}}))
        .deposit(Balance::from_millinear(20)).transact();
    out.assert_panic_contains("nested T2 delegation is unsupported");
    let done = r.complete(root, WORKER, passing());
    let out = r.sim.call(WORKER, CONTRACT, "anchor")
        .args_json(json!({"run_id": "t2", "seq": 2, "actor": WORKER, "kind": "settlement", "payload_hash": digest(passing()), "prev_hash": r.head,
            "task": {"assignment_seq": root, "worker": WORKER, "parent_assignment_seq": null}}))
        .deposit(Balance::from_millinear(20)).transact();
    out.assert_panic_contains("assignment already completed");
    let out = r.sim.call(COORD, CONTRACT, "file_claim")
        .args_json(json!({"run_id": "t2", "blamed": WORKER, "failure_class": "F2", "verifier_hash": r.verifier,
            "evidence_seqs": [root], "evidence_payload_hashes": [digest(&r.bodies[&root])], "evidence_tx_hashes": [r.transactions[&root]]}))
        .deposit(Balance::from_millinear(280)).transact();
    out.assert_panic_contains("unsupported T2 claim class");
    let id = r.file(root, WORKER, "F1");
    r.challenge(id);
    assert_eq!(r.preview(id, &[root, done])["verdict"], "Overturned");
    r.sim.findings().assert_none();
}

impl Run {
    fn persist(&mut self, id: u64, seqs: &[u64]) -> Value {
        let before = self.state(id);
        let expected = self.preview(id, seqs);
        let args = self.args(id, seqs);
        let caller_before = self.sim.balance(CALLER).as_yoctonear();
        let contract_before = self.sim.balance(CONTRACT);
        let out = self
            .sim
            .call(CALLER, CONTRACT, "adjudicate")
            .args_json(args)
            .gas(Gas::from_teragas(100))
            .transact();
        out.assert_success();
        let result: Value = out.json();
        assert_eq!(result, expected);
        assert!(out.effects().near.is_empty());
        assert!(out.effects().pending.is_empty());
        assert_eq!(out.effects().storage_bytes(CONTRACT), 0);
        assert_eq!(self.sim.balance(CONTRACT), contract_before);
        assert_eq!(
            caller_before - self.sim.balance(CALLER).as_yoctonear(),
            out.effects().gas_paid().as_yoctonear()
        );
        let after = self.state(id);
        for field in ["run", "worker_rep", "coordinator_rep", "treasury"] {
            assert_eq!(after[field], before[field]);
        }
        for field in [
            "bond",
            "challenge_bond",
            "challenger",
            "evidence_snapshot",
            "challenge_deadline",
        ] {
            assert_eq!(after["claim"][field], before["claim"][field]);
        }
        assert_eq!(after["claim"]["adjudication_result"], result);
        assert_eq!(
            after["claim"]["corrected_attribution"],
            result["finding"]["agent"]
        );
        assert_eq!(
            after["claim"]["status"],
            if result["verdict"] == "Upheld" {
                "AdjudicatedUpheld"
            } else {
                "AdjudicatedOverturned"
            }
        );
        result
    }

    fn state_value(&self) -> Value {
        self.sim
            .view(CONTRACT, "get_run", json!({"run_id": "t2"}))
            .json()
    }
}

#[test]
fn persisted_causal_verdicts_permissionless_and_replay() {
    let longest = "w".repeat(64);
    for (body, output, class, blamed, worker, challenger, verdict, fault, agent, origin) in [
        (
            input(),
            passing(),
            "F1",
            WORKER,
            WORKER,
            WORKER,
            "Overturned",
            None,
            None,
            None,
        ),
        (
            input(),
            b"wrong".as_slice(),
            "F1",
            WORKER,
            WORKER,
            WORKER,
            "Upheld",
            Some("F1"),
            Some(WORKER),
            Some("Introduction"),
        ),
        (
            b"malformed".to_vec(),
            passing(),
            "F3",
            COORD,
            WORKER,
            WORKER,
            "Upheld",
            Some("F3"),
            Some(COORD),
            Some("Introduction"),
        ),
        (
            b"malformed".to_vec(),
            b"wrong".as_slice(),
            "F1",
            WORKER,
            WORKER,
            WORKER,
            "Overturned",
            Some("F3"),
            Some(COORD),
            Some("Propagation"),
        ),
        (
            b"malformed".to_vec(),
            b"wrong".as_slice(),
            "F1",
            WORKER,
            WORKER,
            REVIEWER,
            "Overturned",
            Some("F3"),
            Some(COORD),
            Some("Propagation"),
        ),
        (
            input(),
            b"wrong".as_slice(),
            "F1",
            REVIEWER,
            WORKER,
            WORKER,
            "Overturned",
            Some("F1"),
            Some(WORKER),
            Some("Introduction"),
        ),
        (
            input(),
            b"wrong".as_slice(),
            "F1",
            REVIEWER,
            longest.as_str(),
            REVIEWER,
            "Overturned",
            Some("F1"),
            Some(longest.as_str()),
            Some("Introduction"),
        ),
        (
            input(),
            passing(),
            "F1",
            longest.as_str(),
            longest.as_str(),
            REVIEWER,
            "Overturned",
            None,
            None,
            None,
        ),
    ] {
        let spec =
            serde_json::to_vec(&json!({"task_type": "t2_fee_split_v1", "fee_basis_points": 250}))
                .unwrap();
        let mut r = Run::with_worker(spec, worker);
        let root = r.assign(worker, &body);
        let done = r.complete(root, worker, output);
        let id = r.file(root, blamed, class);
        r.sim
            .call(challenger, CONTRACT, "challenge")
            .args_json(json!({"claim_id": id}))
            .deposit(Balance::from_millinear(270))
            .transact()
            .assert_success();
        let result = r.persist(id, &[done, root]);
        assert_eq!(result["verdict"], verdict);
        assert_eq!(
            result["finding"]["failure_class"],
            fault.map_or(Value::Null, |s| json!(s))
        );
        assert_eq!(
            result["finding"]["agent"],
            agent.map_or(Value::Null, |s| json!(s))
        );
        assert_eq!(result["origin"], origin.map_or(Value::Null, |s| json!(s)));
        if fault.is_some() {
            assert_eq!(result["finding"]["origin"], "Introduction");
        }
        assert_eq!(
            r.sim
                .view(CONTRACT, "get_reputation", json!({"agent": worker}))
                .json::<u8>(),
            100
        );
        let frozen = r.state(id);
        // A decodable replay bypasses evidence validation, even for oversized or wrong bytes.
        let replay = r
            .sim
            .call(COORD, CONTRACT, "adjudicate")
            .args_json(json!({"claim_id": id, "task_spec": STANDARD.encode(b"{}"),
                "payloads": [{"seq": 999, "bytes": STANDARD.encode(vec![0u8; 8193])}]}))
            .transact();
        replay.assert_success();
        assert_eq!(replay.json::<Value>(), result);
        assert!(replay.effects().near.is_empty());
        assert_eq!(replay.effects().storage_bytes(CONTRACT), 0);
        assert_eq!(r.state(id), frozen);
        r.sim
            .call(WORKER, CONTRACT, "finalize")
            .args_json(json!({"claim_id": id}))
            .transact()
            .assert_panic_contains("challenge window still open");
        r.sim
            .call(WORKER, CONTRACT, "challenge")
            .args_json(json!({"claim_id": id}))
            .deposit(Balance::from_millinear(270))
            .transact()
            .assert_panic_contains("claim must be pending");
        let args = r.args(id, &[root, done]);
        r.sim
            .call(CALLER, CONTRACT, "adjudicate")
            .args_json(args)
            .deposit(Balance::from_yoctonear(1))
            .transact()
            .assert_panic_contains("doesn't accept deposit");
        assert_eq!(r.state(id), frozen);
        r.sim.findings().assert_none();
    }
}

#[test]
fn persisted_clean_task_ignores_failed_sibling() {
    let mut r = Run::new(250);
    let bad = r.assign(REVIEWER, b"invalid unrelated input");
    r.complete(bad, REVIEWER, b"invalid unrelated result");
    let root = r.assign(WORKER, &input());
    let done = r.complete(root, WORKER, passing());
    let id = r.file(root, WORKER, "F1");
    r.challenge(id);
    let result = r.persist(id, &[root, done]);
    assert_eq!(result["verdict"], "Overturned");
    assert_eq!(result["finding"], Value::Null);
    assert_eq!(r.state(id)["claim"]["corrected_attribution"], Value::Null);
    r.sim.findings().assert_none();
}

#[test]
fn challenge_funds_result_slot_and_adjudication_state_boundaries() {
    let mut r = Run::new(250);
    let root = r.assign(WORKER, &input());
    let done = r.complete(root, WORKER, passing());
    let id = r.file(root, WORKER, "F1");
    let before = r.state(id);
    let args = r.args(id, &[root, done]);
    r.sim
        .call(CALLER, CONTRACT, "adjudicate")
        .args_json(args)
        .transact()
        .assert_panic_contains("claim must be challenged");
    let args = r.args(999, &[root, done]);
    r.sim
        .call(CALLER, CONTRACT, "adjudicate")
        .args_json(args)
        .transact()
        .assert_panic_contains("unknown claim");
    let rejected = r
        .sim
        .call(WORKER, CONTRACT, "challenge")
        .args_json(json!({"claim_id": id}))
        .deposit(Balance::from_millinear(250))
        .transact();
    rejected.assert_panic_contains("insufficient challenge bond and storage deposit");
    assert_eq!(rejected.effects().near_net(CONTRACT), Delta::ZERO);
    assert_eq!(rejected.effects().storage_bytes(CONTRACT), 0);
    assert_eq!(r.state(id), before);
    let challenged = r
        .sim
        .call(WORKER, CONTRACT, "challenge")
        .args_json(json!({"claim_id": id}))
        .deposit(Balance::from_millinear(270))
        .transact();
    challenged.assert_success();
    let storage = r
        .sim
        .storage_cost(u64::try_from(challenged.effects().storage_bytes(CONTRACT)).unwrap())
        .as_yoctonear();
    let required = Balance::from_millinear(250).as_yoctonear() + storage;
    assert_eq!(
        challenged.effects().near_net(CONTRACT),
        Delta::gain(required)
    );
    assert_eq!(
        challenged
            .effects()
            .near_paid(CONTRACT, WORKER)
            .as_yoctonear(),
        Balance::from_millinear(270).as_yoctonear() - required
    );
    let funded = r.state(id);
    assert_eq!(funded["claim"]["challenge_bond"], before["claim"]["bond"]);
    // A repeated challenger reuses its prepaid per-run marker, so measure the
    // smaller repeated-call storage charge before checking exact funding.
    let warm_id = r.file(root, WORKER, "F1");
    let warm = r
        .sim
        .call(WORKER, CONTRACT, "challenge")
        .args_json(json!({"claim_id":warm_id}))
        .deposit(Balance::from_millinear(270))
        .transact();
    warm.assert_success();
    let repeated_storage = r
        .sim
        .storage_cost(u64::try_from(warm.effects().storage_bytes(CONTRACT)).unwrap())
        .as_yoctonear();
    assert!(repeated_storage < storage);
    let required = Balance::from_millinear(250).as_yoctonear() + repeated_storage;
    let exact_id = r.file(root, WORKER, "F1");
    let exact = r
        .sim
        .call(WORKER, CONTRACT, "challenge")
        .args_json(json!({"claim_id": exact_id}))
        .deposit(Balance::from_yoctonear(required))
        .transact();
    exact.assert_success();
    assert_eq!(
        exact.effects().near_paid(CONTRACT, WORKER).as_yoctonear(),
        0
    );
    assert_eq!(exact.effects().near_net(CONTRACT), Delta::gain(required));
    let final_id = r.file(root, WORKER, "F1");
    let deadline = r.state(final_id)["claim"]["challenge_deadline"]
        .as_u64()
        .unwrap();
    r.sim.advance_to(deadline + 1);
    assert_eq!(r.persist(id, &[root, done])["verdict"], "Overturned");
    assert_eq!(r.persist(exact_id, &[root, done])["verdict"], "Overturned");
    r.sim
        .call(COORD, CONTRACT, "finalize")
        .args_json(json!({"claim_id": final_id}))
        .transact()
        .assert_success();
    let finalized = r.state(final_id);
    let args = r.args(final_id, &[root, done]);
    r.sim
        .call(CALLER, CONTRACT, "adjudicate")
        .args_json(args)
        .transact()
        .assert_panic_contains("claim must be challenged");
    assert_eq!(r.state(final_id), finalized);
    r.sim.findings().assert_none();
}

#[test]
fn bounded_evidence_and_100_event_wasm_gas_measurement() {
    let mut spec =
        serde_json::to_vec(&json!({"task_type": "t2_fee_split_v1", "fee_basis_points": 250}))
            .unwrap();
    spec.resize(1024, b' ');
    let mut r = Run::with_spec(spec);
    let maximum = u128::MAX;
    let mut entries = vec![(maximum / 128).to_string(); 128];
    entries[127] = (maximum / 128 + maximum % 128).to_string();
    let mut body =
        serde_json::to_vec(&json!({"entries": entries, "declared_total": maximum.to_string()}))
            .unwrap();
    body.resize(8192, b' ');
    let root = r.assign(WORKER, &body);
    let done = r.complete(root, WORKER, &vec![b'x'; 8192]);
    while r.bodies.len() < 100 {
        r.append(COORD, "note", b"unrelated", None);
    }
    let id = r.file(root, WORKER, "F1");
    r.challenge(id);
    let before = r.state(id);
    let args = r.args(id, &[root, done]);
    let out = r
        .sim
        .call(CALLER, CONTRACT, "preview_adjudication")
        .args_json(args.clone())
        .gas(Gas::from_teragas(100))
        .transact();
    out.assert_success();
    assert_eq!(out.json::<Value>()["verdict"], "Upheld");
    assert_eq!(out.json::<Value>()["finding"]["failure_class"], "F1");
    assert_eq!(r.state(id), before);
    assert!(out.effects().near.is_empty());
    let metrics = json!({"environment": "nearcore 2.14.0 simulator; not measured testnet pricing",
        "events": 100, "spec_bytes": 1024, "input_bytes": 8192, "output_bytes": 8192, "ledger_entries": 128, "wire_bytes": serde_json::to_vec(&args).unwrap().len(),
        "gas_burnt": out.gas_burnt().as_gas(), "gas_paid_yocto": out.effects().gas_paid().as_yoctonear().to_string(),
        "attached_gas": 100_000_000_000_000u64});
    std::fs::write(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../codex-log/2.18-preview-stress.json"),
        serde_json::to_vec_pretty(&metrics).unwrap(),
    )
    .unwrap();
    let published: Value = r.sim.view(CONTRACT, "get_verifier_info", json!({})).json();
    std::fs::write(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../codex-log/2.18-verifier-info.json"),
        serde_json::to_vec_pretty(&published).unwrap(),
    )
    .unwrap();
    println!("Measured preview gas: {metrics}");
    let mut too_big = args.clone();
    too_big["payloads"][0]["bytes"] = json!(STANDARD.encode(vec![0u8; 8193]));
    r.sim
        .call(CALLER, CONTRACT, "adjudicate")
        .args_json(too_big)
        .transact()
        .assert_panic_contains("payload exceeds byte limit");
    assert_eq!(r.state(id), before);
    let mut too_big = args.clone();
    too_big["payloads"][0]["bytes"] = json!(STANDARD.encode(vec![0u8; 50_000]));
    r.sim
        .call(CALLER, CONTRACT, "adjudicate")
        .args_json(too_big)
        .transact()
        .assert_panic_contains("adjudication call exceeds byte limit");
    assert_eq!(r.state(id), before);
    let caller_before = r.sim.balance(CALLER).as_yoctonear();
    let persisted = r
        .sim
        .call(CALLER, CONTRACT, "adjudicate")
        .args_json(args.clone())
        .gas(Gas::from_teragas(100))
        .transact();
    persisted.assert_success();
    assert_eq!(persisted.json::<Value>(), out.json::<Value>());
    assert!(persisted.effects().near.is_empty());
    assert_eq!(persisted.effects().storage_bytes(CONTRACT), 0);
    assert_eq!(
        caller_before - r.sim.balance(CALLER).as_yoctonear(),
        persisted.effects().gas_paid().as_yoctonear()
    );
    let mut persisted_metrics = metrics;
    persisted_metrics["gas_burnt"] = json!(persisted.gas_burnt().as_gas());
    persisted_metrics["gas_paid_yocto"] =
        json!(persisted.effects().gas_paid().as_yoctonear().to_string());
    persisted_metrics["storage_delta_bytes"] = json!(persisted.effects().storage_bytes(CONTRACT));
    std::fs::write(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../codex-log/2.18-sim-gas.json"),
        serde_json::to_vec_pretty(&persisted_metrics).unwrap(),
    )
    .unwrap();
    println!("Measured adjudication gas: {persisted_metrics}");
    let mut too_big = args;
    let mut too_big_call = too_big.clone();
    too_big_call["payloads"][0]["bytes"] = json!(STANDARD.encode(vec![0u8; 50_000]));
    r.sim
        .view(CONTRACT, "preview_adjudication", too_big_call)
        .assert_panic_contains("adjudication call exceeds byte limit");
    too_big["payloads"][0]["bytes"] = json!(STANDARD.encode(vec![0u8; 8193]));
    r.sim
        .view(CONTRACT, "preview_adjudication", too_big)
        .assert_panic_contains("payload exceeds byte limit");
    let out = r.sim.call(COORD, CONTRACT, "anchor")
        .args_json(json!({"run_id": "t2", "seq": 100, "actor": COORD, "kind": "note", "payload_hash": digest(b"extra"), "prev_hash": r.head}))
        .deposit(Balance::from_millinear(20)).transact();
    out.assert_panic_contains("embedded verifier event limit reached");
    r.sim.findings().assert_none();
}

fn amount(v: &Value) -> u128 {
    v.as_str().unwrap().parse().unwrap()
}
fn rep(r: &Run, who: &str) -> u8 {
    r.sim
        .view(CONTRACT, "get_reputation", json!({"agent": who}))
        .json()
}

fn expire(r: &mut Run, id: u64) {
    let before = r.state(id);
    let deadline = before["claim"]["evidence_snapshot"]["challenge_height"]
        .as_u64()
        .unwrap()
        + 600;
    r.sim.advance_to(r.sim.height().max(deadline + 1));
    let out = r
        .sim
        .call(CALLER, CONTRACT, "resolve_expired")
        .args_json(json!({"claim_id": id}))
        .transact();
    out.assert_success();
    assert_eq!(out.json::<Value>(), "AdjudicatedOverturned");
    assert!(out.effects().near.is_empty());
    assert_eq!(out.effects().storage_bytes(CONTRACT), 0);
    let mut expected = before;
    expected["claim"]["status"] = json!("AdjudicatedOverturned");
    assert_eq!(r.state(id), expected);
    assert!(expected["claim"]["adjudication_result"].is_null());
    assert!(expected["claim"]["corrected_attribution"].is_null());
}

#[test]
fn expiry_strict_boundary_replay_and_terminal_rejections() {
    let mut r = Run::new(250);
    let root = r.assign(WORKER, &input());
    let done = r.complete(root, WORKER, passing());
    let id = r.file(root, WORKER, "F1");
    for claim_id in [id, 999] {
        r.sim
            .call(CALLER, CONTRACT, "resolve_expired")
            .args_json(json!({"claim_id": claim_id}))
            .transact()
            .assert_panic_contains(if claim_id == id {
                "claim must be challenged"
            } else {
                "unknown claim"
            });
    }
    r.challenge(id);
    let frozen = r.state(id);
    r.sim
        .call(CALLER, CONTRACT, "resolve_expired")
        .args_json(json!({"claim_id": id}))
        .transact()
        .assert_panic_contains("adjudication window still open");
    assert_eq!(r.state(id), frozen);
    let deadline = frozen["claim"]["evidence_snapshot"]["challenge_height"]
        .as_u64()
        .unwrap()
        + 600;
    r.sim.advance_to(deadline - 2);
    let boundary = r
        .sim
        .call(CALLER, CONTRACT, "resolve_expired")
        .args_json(json!({"claim_id": id}))
        .submit();
    r.sim.step();
    let after = r
        .sim
        .call(REVIEWER, CONTRACT, "resolve_expired")
        .args_json(json!({"claim_id": id}))
        .submit();
    r.sim.settle();
    let boundary = r.sim.outcome(boundary);
    boundary.assert_panic_contains("adjudication window still open");
    assert_eq!(
        boundary.receipt("resolve_expired").unwrap().block_height,
        deadline
    );
    let after = r.sim.outcome(after);
    after.assert_success();
    assert_eq!(
        after.receipt("resolve_expired").unwrap().block_height,
        deadline + 1
    );
    assert_eq!(after.json::<Value>(), "AdjudicatedOverturned");
    assert!(after.effects().near.is_empty());
    assert_eq!(after.effects().storage_bytes(CONTRACT), 0);
    let resolved = r.state(id);
    let replay = r
        .sim
        .call(COORD, CONTRACT, "resolve_expired")
        .args_json(json!({"claim_id": id}))
        .transact();
    replay.assert_success();
    assert_eq!(replay.json::<Value>(), "AdjudicatedOverturned");
    assert!(replay.effects().near.is_empty());
    assert_eq!(replay.effects().storage_bytes(CONTRACT), 0);
    assert_eq!(r.state(id), resolved);
    // Expiry wins: even complete, valid evidence cannot replace the default.
    let complete_args = r.args(id, &[root, done]);
    r.sim
        .call(CALLER, CONTRACT, "adjudicate")
        .args_json(complete_args)
        .transact()
        .assert_panic_contains("claim must be challenged");
    assert_eq!(r.state(id), resolved);
    let n = Balance::from_near(1).as_yoctonear();
    finalize_checked(&mut r, id, WORKER, 45 * n / 100, 5 * n / 100, 0);
    let finalized = r.state(id);
    r.sim
        .call(CALLER, CONTRACT, "resolve_expired")
        .args_json(json!({"claim_id": id}))
        .transact()
        .assert_panic_contains("claim must be challenged");
    assert_eq!(r.state(id), finalized);
}

#[test]
fn expiry_unproved_claims_use_case_three_without_inventing_attribution() {
    let n = Balance::from_near(1).as_yoctonear();
    // Completion absent, completion bytes unavailable, assignment unavailable,
    // both bodies unavailable, and S6 with known invalid input: same default.
    // These describe test caller inputs, not on-chain withholding detection.
    for scenario in 0..5 {
        let mut r = Run::new(250);
        let body = if scenario == 4 {
            b"malformed".to_vec()
        } else {
            input()
        };
        let root = r.assign(WORKER, &body);
        let done = (scenario != 0).then(|| r.complete(root, WORKER, b"wrong"));
        let id = r.file(root, WORKER, "F1");
        r.sim
            .call(REVIEWER, CONTRACT, "challenge")
            .args_json(json!({"claim_id": id}))
            .deposit(Balance::from_millinear(280))
            .transact()
            .assert_success();
        let frozen = r.state(id);
        if scenario == 1 || scenario == 2 {
            let supplied = if scenario == 1 {
                vec![root]
            } else {
                vec![done.unwrap()]
            };
            let partial_args = r.args(id, &supplied);
            r.sim
                .call(CALLER, CONTRACT, "adjudicate")
                .args_json(partial_args)
                .transact()
                .assert_panic_contains("incomplete task evidence");
            assert_eq!(r.state(id), frozen);
        }
        if scenario == 4 {
            let preview = r.preview(id, &[root, done.unwrap()]);
            assert_eq!(preview["finding"]["agent"], COORD);
            assert_eq!(preview["origin"], "Propagation");
        }
        // Appending after challenge cannot change the expiry basis or deadline.
        r.append(REVIEWER, "note", b"late", None);
        expire(&mut r, id);
        finalize_checked(&mut r, id, REVIEWER, 45 * n / 100, 5 * n / 100, 0);
        assert_eq!(
            [COORD, WORKER, REVIEWER].map(|a| rep(&r, a)),
            [85, 102, 105]
        );
    }
}

#[test]
fn expiry_preserves_adjudicated_outcomes_before_and_after_deadline() {
    for late in [false, true] {
        for upheld in [false, true] {
            let mut r = Run::new(250);
            let root = r.assign(WORKER, &input());
            let done = r.complete(root, WORKER, if upheld { b"wrong" } else { passing() });
            let id = r.file(root, WORKER, "F1");
            r.challenge(id);
            if late {
                let height = r.state(id)["claim"]["evidence_snapshot"]["challenge_height"]
                    .as_u64()
                    .unwrap();
                r.sim.advance_to(height + 601);
            }
            let verified = r.persist(id, &[root, done]);
            let frozen = r.state(id);
            let replay = r
                .sim
                .call(REVIEWER, CONTRACT, "resolve_expired")
                .args_json(json!({"claim_id": id}))
                .transact();
            replay.assert_success();
            assert_eq!(
                replay.json::<Value>(),
                if upheld {
                    "AdjudicatedUpheld"
                } else {
                    "AdjudicatedOverturned"
                }
            );
            assert_eq!(r.state(id), frozen);
            assert!(replay.effects().near.is_empty());
            assert_eq!(replay.effects().storage_bytes(CONTRACT), 0);
            assert_eq!(r.state(id)["claim"]["adjudication_result"], verified);
            let n = Balance::from_near(1).as_yoctonear();
            if upheld {
                finalize_checked(&mut r, id, COORD, 65 * n / 100, n / 10, n / 4);
                assert_eq!([COORD, WORKER].map(|a| rep(&r, a)), [100, 90]);
            } else {
                finalize_checked(&mut r, id, WORKER, 45 * n / 100, 5 * n / 100, 0);
                assert_eq!([COORD, WORKER].map(|a| rep(&r, a)), [85, 107]);
            }
        }
    }
}

#[test]
fn expiry_challenger_bonus_is_consumed_once_per_run_and_roles_accumulate() {
    let n = Balance::from_near(1).as_yoctonear();
    let mut r = Run::new(250);
    let root = r.assign(WORKER, &input());
    for i in 0..2 {
        let id = r.file(root, WORKER, "F1");
        r.sim
            .call(REVIEWER, CONTRACT, "challenge")
            .args_json(json!({"claim_id": id}))
            .deposit(Balance::from_millinear(280))
            .transact()
            .assert_success();
        expire(&mut r, id);
        finalize_checked(&mut r, id, REVIEWER, 45 * n / 100, 5 * n / 100, 0);
        assert_eq!(
            [COORD, WORKER, REVIEWER].map(|a| rep(&r, a)),
            [85 - 15 * i, 102 + 2 * i, 105]
        );
    }
    let id = r.file_as(WORKER, root, WORKER, "F1");
    r.challenge(id);
    expire(&mut r, id);
    finalize_checked(&mut r, id, WORKER, 45 * n / 100, 5 * n / 100, 0);
    assert_eq!(rep(&r, WORKER), 96); // 104 - 15 + 5 + 2; no new culprit penalty
}

fn finalize_checked(
    r: &mut Run,
    id: u64,
    recipient: &str,
    payout: u128,
    treasury_gain: u128,
    stake_loss: u128,
) {
    let before = r.state(id);
    let stakes = |run: &Value| -> u128 {
        run["agents"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| amount(&a["locked_stake"]))
            .sum()
    };
    let old_stakes = stakes(&before["run"]);
    let balance = r.sim.balance(recipient).as_yoctonear();
    let deadline = before["claim"]["challenge_deadline"].as_u64().unwrap();
    r.sim.advance_to(r.sim.height().max(deadline + 1));
    // sponsor is distinct from all payout roles in these fixtures, isolating gas.
    let out = r
        .sim
        .call("sponsor.near", CONTRACT, "finalize")
        .args_json(json!({"claim_id": id}))
        .transact();
    out.assert_success();
    let effects = out.effects();
    assert_eq!(
        effects.near_paid(CONTRACT, recipient).as_yoctonear(),
        payout
    );
    assert_eq!(effects.near_net(CONTRACT), Delta::loss(payout));
    for who in [COORD, WORKER, REVIEWER, CALLER, "sponsor.near"] {
        if who != recipient {
            assert_eq!(effects.near_net(who), Delta::ZERO);
        }
    }
    assert_eq!(effects.near_net(recipient), Delta::gain(payout));
    assert_eq!(r.sim.balance(recipient).as_yoctonear(), balance + payout);
    assert_eq!(effects.storage_bytes(CONTRACT), 0);
    assert_eq!(effects.near.len(), 1);
    assert!(effects.pending.is_empty());
    let after = r.state(id);
    assert_eq!(after["claim"]["status"], "Finalized");
    assert_eq!(after["claim"]["bond"], "0");
    assert_eq!(after["claim"]["challenge_bond"], "0");
    assert_eq!(old_stakes - stakes(&after["run"]), stake_loss);
    assert_eq!(
        amount(&after["treasury"]) - amount(&before["treasury"]),
        treasury_gain
    );
    assert_eq!(
        amount(&before["claim"]["bond"]) + amount(&before["claim"]["challenge_bond"]) + stake_loss,
        payout + treasury_gain
    );
    assert_eq!(before["run"]["events"], after["run"]["events"]);
    assert_eq!(
        before["claim"]["adjudication_result"],
        after["claim"]["adjudication_result"]
    );
    let scores: Vec<_> = [COORD, WORKER, REVIEWER, CALLER]
        .map(|a| rep(r, a))
        .to_vec();
    let replay = r
        .sim
        .call(CALLER, CONTRACT, "finalize")
        .args_json(json!({"claim_id": id}))
        .transact();
    replay.assert_success();
    assert!(replay.effects().near.is_empty());
    assert_eq!(replay.effects().storage_bytes(CONTRACT), 0);
    assert_eq!(r.state(id), after);
    assert_eq!(
        [COORD, WORKER, REVIEWER, CALLER]
            .map(|a| rep(r, a))
            .to_vec(),
        scores
    );
    r.sim.findings().assert_none();
}

#[test]
fn finalize_four_cases_s2_s5_s6_and_overlapping_roles() {
    let n = Balance::from_near(1).as_yoctonear();
    // input valid, output valid, blamed, reporter, challenger, expected scores C/W/R/caller.
    for (valid_input, valid_output, blamed, reporter, challenger, expected) in [
        (true, false, WORKER, COORD, None, [100, 90, 100, 100]), // case 1
        (true, true, WORKER, COORD, None, [100, 90, 100, 100]),  // case 4
        (
            true,
            false,
            WORKER,
            COORD,
            Some(REVIEWER),
            [100, 90, 100, 100],
        ), // case 2
        (
            false,
            true,
            COORD,
            COORD,
            Some(REVIEWER),
            [90, 100, 100, 100],
        ), // S5
        (
            true,
            true,
            WORKER,
            COORD,
            Some(REVIEWER),
            [85, 102, 105, 100],
        ), // S2 watcher
        (true, true, WORKER, COORD, Some(WORKER), [85, 107, 100, 100]), // S2 self
        (
            false,
            false,
            WORKER,
            COORD,
            Some(REVIEWER),
            [85, 102, 105, 100],
        ), // S6 watcher, U8
        (
            false,
            false,
            WORKER,
            COORD,
            Some(WORKER),
            [85, 107, 100, 100],
        ), // S6 self
        (
            true,
            true,
            WORKER,
            CALLER,
            Some(REVIEWER),
            [100, 102, 105, 85],
        ), // outsider reporter
        (true, true, WORKER, COORD, Some(CALLER), [85, 102, 100, 105]), // outsider challenger
        (true, true, WORKER, COORD, Some(COORD), [90, 102, 100, 100]), // reporter=challenger
        (
            true,
            true,
            WORKER,
            WORKER,
            Some(WORKER),
            [100, 92, 100, 100],
        ), // all roles equal
    ] {
        let mut r = Run::new(250);
        let root = r.assign(
            WORKER,
            &if valid_input {
                input()
            } else {
                b"malformed".to_vec()
            },
        );
        let done = r.complete(
            root,
            WORKER,
            if valid_output { passing() } else { b"wrong" },
        );
        let class = if blamed == COORD { "F3" } else { "F1" };
        let id = r.file_as(reporter, root, blamed, class);
        let mut upheld = true;
        if let Some(challenger) = challenger {
            r.sim
                .call(challenger, CONTRACT, "challenge")
                .args_json(json!({"claim_id":id}))
                .deposit(Balance::from_millinear(280))
                .transact()
                .assert_success();
            let frozen = r.state(id);
            let reject = r
                .sim
                .call(CALLER, CONTRACT, "finalize")
                .args_json(json!({"claim_id":id}))
                .transact();
            reject.assert_panic_contains("claim is not ready to finalize");
            assert_eq!(r.state(id), frozen);
            let result = r.persist(id, &[root, done]);
            upheld = result["verdict"] == "Upheld";
        }
        let frozen = r.state(id);
        let early = r
            .sim
            .call(CALLER, CONTRACT, "finalize")
            .args_json(json!({"claim_id":id}))
            .transact();
        early.assert_panic_contains("challenge window still open");
        assert_eq!(r.state(id), frozen);
        let (receiver, payout, treasury, slash) = match challenger {
            None => (reporter, 45 * n / 100, 5 * n / 100, n / 4),
            Some(_) if upheld => (reporter, 65 * n / 100, n / 10, n / 4),
            Some(challenger) => (challenger, 45 * n / 100, 5 * n / 100, 0),
        };
        finalize_checked(&mut r, id, receiver, payout, treasury, slash);
        assert_eq!(
            [COORD, WORKER, REVIEWER, CALLER].map(|a| rep(&r, a)),
            expected
        );
    }
}

#[test]
fn finalize_stake_exhaustion_cap_and_partial_upheld() {
    let n = Balance::from_near(1).as_yoctonear();
    let mut r = Run::new(250);
    let root = r.assign(WORKER, &input());
    let done = r.complete(root, WORKER, b"wrong");
    // Partial remaining stake uses the filed bond, despite later parameter changes.
    r.sim.call("sponsor.near",CONTRACT,"update_parameters").args_json(json!({"stake": n.to_string(),"blame_bond":(3*n/10).to_string(),"challenge_bond":(3*n/10).to_string()})).transact().assert_success();
    for i in 0..5 {
        // Larger deposit for the 0.30 bond.
        let id: u64 = r.sim.call(COORD,CONTRACT,"file_claim").args_json(json!({"run_id":"t2","blamed":WORKER,"failure_class":"F1","verifier_hash":r.verifier,"evidence_seqs":[root],"evidence_payload_hashes":[digest(&r.bodies[&root])],"evidence_tx_hashes":[r.transactions[&root]]})).deposit(Balance::from_millinear(350)).transact().json();
        let s = if i < 3 {
            3 * n / 10
        } else if i == 3 {
            n / 10
        } else {
            0
        };
        if i == 3 {
            r.sim
                .call(REVIEWER, CONTRACT, "challenge")
                .args_json(json!({"claim_id":id}))
                .deposit(Balance::from_millinear(350))
                .transact()
                .assert_success();
            r.persist(id, &[root, done]);
            finalize_checked(&mut r, id, COORD, 62 * n / 100, 8 * n / 100, s);
        } else {
            finalize_checked(&mut r, id, COORD, 3 * n / 10 + s * 4 / 5, s / 5, s);
        }
    }
    assert_eq!(rep(&r, WORKER), 50);
    // Exhausted stake still consumes the full challenger bond on upheld.
    r.sim.call("sponsor.near",CONTRACT,"update_parameters").args_json(json!({"stake":n.to_string(),"blame_bond":(n/4).to_string(),"challenge_bond":(n/4).to_string()})).transact().assert_success();
    let id = r.file(root, WORKER, "F1");
    r.challenge(id);
    r.persist(id, &[root, done]);
    finalize_checked(&mut r, id, COORD, 45 * n / 100, 5 * n / 100, 0);
    assert_eq!(rep(&r, WORKER), 40);
}

#[test]
fn finalize_bonus_cap_and_reputation_saturation() {
    let n = Balance::from_near(1).as_yoctonear();
    let mut r = Run::new(250);
    let root = r.assign(WORKER, &input());
    let done = r.complete(root, WORKER, passing());
    for i in 0..12 {
        let id = r.file(root, WORKER, "F1");
        r.sim
            .call(REVIEWER, CONTRACT, "challenge")
            .args_json(json!({"claim_id":id}))
            .deposit(Balance::from_millinear(280))
            .transact()
            .assert_success();
        r.persist(id, &[root, done]);
        finalize_checked(&mut r, id, REVIEWER, 45 * n / 100, 5 * n / 100, 0);
        assert_eq!(rep(&r, REVIEWER), 105); // only the first win grants +5
        assert_eq!(rep(&r, COORD), 100u8.saturating_sub(15 * (i + 1)));
    }
    assert_eq!(rep(&r, WORKER), 120);
    assert_eq!(rep(&r, COORD), 0);
    // First worker win is clipped at 120 but still consumes its entitlement.
    let id = r.file(root, WORKER, "F1");
    r.challenge(id);
    r.persist(id, &[root, done]);
    finalize_checked(&mut r, id, WORKER, 45 * n / 100, 5 * n / 100, 0);
    assert_eq!(rep(&r, WORKER), 120);
    let id = r.file_as(WORKER, root, REVIEWER, "F1");
    r.sim
        .call(REVIEWER, CONTRACT, "challenge")
        .args_json(json!({"claim_id":id}))
        .deposit(Balance::from_millinear(280))
        .transact()
        .assert_success();
    r.persist(id, &[root, done]);
    finalize_checked(&mut r, id, REVIEWER, 45 * n / 100, 5 * n / 100, 0);
    assert_eq!(rep(&r, WORKER), 105);
    let id = r.file(root, WORKER, "F1");
    r.challenge(id);
    r.persist(id, &[root, done]);
    finalize_checked(&mut r, id, WORKER, 45 * n / 100, 5 * n / 100, 0);
    assert_eq!(rep(&r, WORKER), 107); // exoneration only, no deferred clipped bonus
}

#[test]
fn finalize_odd_bond_snapshot_and_adjudicated_deadline_boundary() {
    let mut r = Run::new(250);
    let root = r.assign(WORKER, &input());
    let done = r.complete(root, WORKER, b"wrong");
    for (bond, challenged, payout, treasury) in [(7u128, false, 12u128, 2u128), (7, true, 18, 3)] {
        r.sim.call("sponsor.near",CONTRACT,"update_parameters").args_json(json!({"stake":Balance::from_near(1).as_yoctonear().to_string(),"blame_bond":bond.to_string(),"challenge_bond":bond.to_string()})).transact().assert_success();
        let id = r.file(root, WORKER, "F1");
        r.sim.call("sponsor.near",CONTRACT,"update_parameters").args_json(json!({"stake":Balance::from_near(1).as_yoctonear().to_string(),"blame_bond":"19","challenge_bond":"19"})).transact().assert_success();
        if challenged {
            r.challenge(id);
            assert_eq!(r.state(id)["claim"]["challenge_bond"], "7");
            r.persist(id, &[root, done]);
        }
        let before = r.state(id);
        let deadline = before["claim"]["challenge_deadline"].as_u64().unwrap();
        r.sim.advance_to(deadline - 2);
        let at = r
            .sim
            .call(CALLER, CONTRACT, "finalize")
            .args_json(json!({"claim_id":id}))
            .submit();
        r.sim.settle();
        let out = r.sim.outcome(at);
        assert_eq!(out.receipt("finalize").unwrap().block_height, deadline);
        out.assert_panic_contains("challenge window still open");
        assert_eq!(r.state(id), before);
        finalize_checked(&mut r, id, COORD, payout, treasury, bond);
    }
}

#[test]
fn finalize_bonus_is_per_run_and_prepaid_outsider_records_rollback() {
    let mut r = Run::new(250);
    let root = r.assign(WORKER, &input());
    let done = r.complete(root, WORKER, passing());
    let id = r.file_as(CALLER, root, WORKER, "F1");
    let before = r.state(id);
    let failed = r
        .sim
        .call(CALLER, CONTRACT, "challenge")
        .args_json(json!({"claim_id":id}))
        .deposit(Balance::from_millinear(250))
        .transact();
    failed.assert_panic_contains("insufficient challenge bond and storage deposit");
    assert_eq!(failed.effects().storage_bytes(CONTRACT), 0);
    assert_eq!(r.state(id), before);
    r.sim
        .call(REVIEWER, CONTRACT, "challenge")
        .args_json(json!({"claim_id":id}))
        .deposit(Balance::from_millinear(280))
        .transact()
        .assert_success();
    r.persist(id, &[root, done]);
    let n = Balance::from_near(1).as_yoctonear();
    finalize_checked(&mut r, id, REVIEWER, 45 * n / 100, 5 * n / 100, 0);
    assert_eq!(rep(&r, REVIEWER), 105);
    // Same account, distinct registered run: bonus eligible again, scores preserved.
    r.sim.call("sponsor.near",CONTRACT,"register").args_json(json!({"run_id":"second","agents":[COORD,WORKER,REVIEWER],"task_spec":STANDARD.encode(&r.spec),"verifier":{"task_spec_hash":digest(&r.spec),"verifier_hash":r.verifier,"verifier_key":null}})).deposit(Balance::from_millinear(3050)).transact().assert_success();
    assert_eq!(rep(&r, REVIEWER), 105);
    let mut head = String::new();
    let mut tx = String::new();
    for (seq, actor, kind, body) in [
        (0, COORD, "task_assigned", input()),
        (1, WORKER, "settlement", passing().to_vec()),
    ] {
        let out=r.sim.call(actor,CONTRACT,"anchor").args_json(json!({"run_id":"second","seq":seq,"actor":actor,"kind":kind,"payload_hash":digest(&body),"prev_hash":head,"task":{"assignment_seq":0,"worker":WORKER,"parent_assignment_seq":null}})).deposit(Balance::from_millinear(20)).transact();
        out.assert_success();
        head = out.json();
        if seq == 0 {
            tx = out.hash.to_string();
        }
    }
    let out=r.sim.call(COORD,CONTRACT,"file_claim").args_json(json!({"run_id":"second","blamed":WORKER,"failure_class":"F1","verifier_hash":r.verifier,"evidence_seqs":[0],"evidence_payload_hashes":[digest(&input())],"evidence_tx_hashes":[tx]})).deposit(Balance::from_millinear(280)).transact();
    out.assert_success();
    let id: u64 = out.json();
    r.sim
        .call(REVIEWER, CONTRACT, "challenge")
        .args_json(json!({"claim_id":id}))
        .deposit(Balance::from_millinear(280))
        .transact()
        .assert_success();
    r.sim.call(CALLER,CONTRACT,"adjudicate").args_json(json!({"claim_id":id,"task_spec":STANDARD.encode(&r.spec),"payloads":[{"seq":0,"bytes":STANDARD.encode(input())},{"seq":1,"bytes":STANDARD.encode(passing())}]})).transact().assert_success();
    let claim: Value = r
        .sim
        .view(CONTRACT, "get_claim", json!({"claim_id":id}))
        .json();
    r.sim
        .advance_to(claim["challenge_deadline"].as_u64().unwrap() + 1);
    let out = r
        .sim
        .call("sponsor.near", CONTRACT, "finalize")
        .args_json(json!({"claim_id":id}))
        .transact();
    out.assert_success();
    assert_eq!(out.effects().storage_bytes(CONTRACT), 0);
    assert_eq!(rep(&r, REVIEWER), 110);
    r.sim.findings().assert_none();
}

#[test]
fn finalize_default_four_slashes_then_fifth_zero() {
    let mut r = Run::new(250);
    let root = r.assign(WORKER, &input());
    r.complete(root, WORKER, b"wrong");
    let n = Balance::from_near(1).as_yoctonear();
    for i in 0..5 {
        let id = r.file(root, WORKER, "F1");
        let slash = if i < 4 { n / 4 } else { 0 };
        finalize_checked(&mut r, id, COORD, n / 4 + slash * 4 / 5, slash / 5, slash);
        assert_eq!(rep(&r, WORKER), 90 - 10 * i);
    }
}
