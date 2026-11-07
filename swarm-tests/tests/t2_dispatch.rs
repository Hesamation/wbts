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
        let mut sim = Sim::new();
        for name in ["sponsor.near", COORD, WORKER, REVIEWER, CALLER] {
            sim.create_account(name, Balance::from_near(100));
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
            .args_json(json!({"run_id": "t2", "agents": [COORD, WORKER, REVIEWER],
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
        let out = self
            .sim
            .call(COORD, CONTRACT, "file_claim")
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
        let out = r
            .sim
            .call(CALLER, CONTRACT, "preview_adjudication")
            .args_json(args)
            .gas(Gas::from_teragas(100))
            .transact();
        out.assert_panic_contains(message);
        assert!(out.effects().near.is_empty());
        assert_eq!(r.state(id), before);
    }
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
    r.sim.findings().assert_none();
}

#[test]
fn matching_metadata_hash_is_not_an_executable_verifier() {
    let mut r = Run::new(250);
    let unknown = "ff".repeat(32);
    r.sim.call("sponsor.near", CONTRACT, "register")
        .args_json(json!({"run_id": "unknown", "agents": [COORD, WORKER],
            "verifier": {"task_spec_hash": digest(&r.spec), "verifier_hash": unknown, "verifier_key": null}}))
        .deposit(Balance::from_millinear(2050)).transact().assert_success();
    let anchored = r
        .sim
        .call(COORD, CONTRACT, "anchor")
        .args_json(
            json!({"run_id": "unknown", "seq": 0, "actor": COORD, "kind": "task_assigned",
            "payload_hash": digest(&input()), "prev_hash": ""}),
        )
        .deposit(Balance::from_millinear(20))
        .transact();
    anchored.assert_success();
    let filed = r.sim.call(COORD, CONTRACT, "file_claim")
        .args_json(json!({"run_id": "unknown", "blamed": WORKER, "failure_class": "F1", "verifier_hash": unknown,
            "evidence_seqs": [0], "evidence_payload_hashes": [digest(&input())], "evidence_tx_hashes": [anchored.hash.to_string()]}))
        .deposit(Balance::from_millinear(280)).transact();
    filed.assert_success();
    let id: u64 = filed.json();
    r.challenge(id);
    let before: Value = r
        .sim
        .view(CONTRACT, "get_claim", json!({"claim_id": id}))
        .json();
    r.sim
        .view(
            CONTRACT,
            "preview_adjudication",
            json!({"claim_id": id, "task_spec": STANDARD.encode(&r.spec),
        "payloads": [{"seq": 0, "bytes": STANDARD.encode(input())}]}),
        )
        .assert_panic_contains("unsupported verifier implementation");
    assert_eq!(
        r.sim
            .view(CONTRACT, "get_claim", json!({"claim_id": id}))
            .json::<Value>(),
        before
    );
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
    fn state_value(&self) -> Value {
        self.sim
            .view(CONTRACT, "get_run", json!({"run_id": "t2"}))
            .json()
    }
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
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../codex-log/2.8-sim-gas.json"),
        serde_json::to_vec_pretty(&metrics).unwrap(),
    )
    .unwrap();
    let published: Value = r.sim.view(CONTRACT, "get_verifier_info", json!({})).json();
    std::fs::write(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../codex-log/2.8-verifier-info.json"),
        serde_json::to_vec_pretty(&published).unwrap(),
    )
    .unwrap();
    println!("Measured gas: {metrics}");
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
