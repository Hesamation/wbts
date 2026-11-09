#![allow(dead_code)]
use base64::{Engine as _, engine::general_purpose::STANDARD};
use near_contract_sim::{Balance, Gas, Sim, TxOutcome};
use serde_json::{Value, json};
use std::sync::OnceLock;
pub const CONTRACT: &str = "engine.testnet";
pub const COORD: &str = "coord.testnet";
pub const WORKER: &str = "worker.testnet";
pub const WATCHER: &str = "watcher.testnet";
pub const OUTSIDER: &str = "outsider.testnet";
pub const N: u128 = 1_000_000_000_000_000_000_000_000;
pub const INPUT: &[u8] = wbts_cli::INPUT;
pub const OUTPUT: &[u8] = wbts_cli::OUTPUT;
pub const BAD: &[u8] = b"bad-input";
pub fn wasm() -> &'static Vec<u8> {
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
        .expect("build current source");
        std::fs::read(path).unwrap()
    })
}
pub fn digest(b: &[u8]) -> String {
    near_contract_sim::nearcore::near_primitives::hash::hash(b)
        .0
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect()
}
pub fn number(v: &Value) -> u128 {
    v.as_str().unwrap().parse().unwrap()
}
pub struct Run {
    pub sim: Sim,
    pub bodies: Vec<Vec<u8>>,
    pub txs: Vec<String>,
}
impl Run {
    pub fn new() -> Self {
        Self::with_bps(250)
    }
    pub fn with_bps(bps: u16) -> Self {
        let mut sim = Sim::new();
        for id in [COORD, WORKER, WATCHER, OUTSIDER] {
            sim.create_account(id, Balance::from_near(100));
        }
        sim.deploy(CONTRACT, wasm().clone());
        sim.call(COORD, CONTRACT, "new")
            .args_json(json!({"demo":true}))
            .transact()
            .assert_success();
        let mut r = Self {
            sim,
            bodies: vec![],
            txs: vec![],
        };
        let spec =
            serde_json::to_vec(&json!({"task_type":"t2_fee_split_v1","fee_basis_points":bps}))
                .unwrap();
        let info = r.view("get_verifier_info", json!({}));
        r.call(COORD,"register",json!({"run_id":"r","agents":[COORD,WORKER,WATCHER],"task_spec":STANDARD.encode(&spec),"verifier":{"task_spec_hash":digest(&spec),"verifier_hash":info["verifier_hash"],"verifier_key":null}}),N/20).assert_success();
        r
    }
    pub fn call(&mut self, signer: &str, method: &str, args: Value, deposit: u128) -> TxOutcome {
        self.sim
            .call(signer, CONTRACT, method)
            .args_json(args)
            .deposit(Balance::from_yoctonear(deposit))
            .gas(Gas::from_teragas(100))
            .transact()
    }
    pub fn view(&self, m: &str, a: Value) -> Value {
        self.sim.view(CONTRACT, m, a).json()
    }
    pub fn id(&self, seq: u64) -> Value {
        json!({"run_id":"r","assignment_seq":seq})
    }
    pub fn edge(&self, seq: u64) -> Value {
        self.view("get_edge", json!({"edge_id":self.id(seq)}))
    }
    pub fn credit(&self, id: &str) -> u128 {
        number(&self.view("get_credit", json!({"account_id":id}))["available"])
    }
    pub fn rep(&self, id: &str) -> u64 {
        self.view("get_reputation", json!({"agent":id}))
            .as_u64()
            .unwrap()
    }
    pub fn offer(&mut self, body: &[u8]) -> u64 {
        self.offer_worker(body, WORKER)
    }
    pub fn offer_worker(&mut self, body: &[u8], worker: &str) -> u64 {
        let seq = self.bodies.len() as u64;
        let h = self.sim.height();
        let head = self.view("get_run", json!({"run_id":"r"}))["head_hash"].clone();
        let out=self.call(COORD,"offer_assignment",json!({"run_id":"r","worker":worker,"payload_hash":digest(body),"accept_deadline":h+100,"work_deadline":h+1000,"seq":seq,"prev_hash":head}),N+N/20);
        out.assert_success();
        self.bodies.push(body.to_vec());
        self.txs.push(out.hash.to_string());
        seq
    }
    pub fn accept(&mut self, seq: u64) {
        let worker = self.edge(seq)["worker"].as_str().unwrap().to_string();
        self.call(
            &worker,
            "accept_assignment",
            json!({"edge_id":self.id(seq)}),
            N + N / 20,
        )
        .assert_success();
    }
    pub fn complete(&mut self, seq: u64, body: &[u8]) {
        let worker = self.edge(seq)["worker"].as_str().unwrap().to_string();
        let n = self.bodies.len();
        let head = self.view("get_run", json!({"run_id":"r"}))["head_hash"].clone();
        let out=self.call(&worker,"anchor",json!({"run_id":"r","seq":n,"actor":worker,"kind":"settlement","payload_hash":digest(body),"prev_hash":head,"task":{"assignment_seq":seq,"worker":worker,"parent_assignment_seq":null}}),N/20);
        out.assert_success();
        self.bodies.push(body.to_vec());
        self.txs.push(out.hash.to_string());
    }
    pub fn args(&self, seq: u64, class: &str) -> Value {
        let edge = self.edge(seq);
        let mut seqs = vec![seq];
        if let Some(n) = edge["completion_seq"].as_u64() {
            seqs.push(n);
        }
        json!({"edge_id":self.id(seq),"failure_class":class,"payloads":seqs.iter().map(|n|json!({"seq":n,"bytes":STANDARD.encode(&self.bodies[*n as usize])})).collect::<Vec<_>>(),"evidence_tx_hashes":seqs.iter().map(|n|self.txs[*n as usize].clone()).collect::<Vec<_>>()})
    }
    pub fn file(&mut self, seq: u64, class: &str, reporter: &str) -> u64 {
        let out = self.call(
            reporter,
            "file_claim",
            self.args(seq, class),
            N / 4 + N / 20,
        );
        out.assert_success();
        out.json()
    }
    pub fn challenge(&mut self, id: u64, by: &str) -> TxOutcome {
        let out = self.call(by, "challenge", json!({"claim_id":id}), N / 4 + N / 20);
        out.assert_success();
        out
    }
    pub fn claim(&self, id: u64) -> Value {
        self.view("get_claim", json!({"claim_id":id}))
    }
    pub fn finalize(&mut self, id: u64) {
        let h = self.claim(id)["challenge_deadline"].as_u64().unwrap();
        self.sim.advance_to(h + 1);
        self.call(OUTSIDER, "finalize", json!({"claim_id":id}), 0)
            .assert_success();
    }
    pub fn release(&mut self, seq: u64) {
        self.call(OUTSIDER, "release_edge", json!({"edge_id":self.id(seq)}), 0)
            .assert_success();
    }
    pub fn wait_release(&mut self, seq: u64) {
        let e = self.edge(seq);
        let normal = e["completion_seq"]
            .as_u64()
            .map(|_| e["completion_height"].as_u64().unwrap())
            .unwrap_or(e["work_deadline"].as_u64().unwrap())
            + e["window"].as_u64().unwrap();
        let h = normal.max(e["corrected_deadline"].as_u64().unwrap()) + 1;
        if self.sim.height() < h {
            self.sim.advance_to(h);
        }
        self.release(seq);
    }
}
