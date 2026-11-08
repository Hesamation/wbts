use near_contract_sim::{Balance, Delta, Sim};
use serde_json::{Value, json};
use std::sync::OnceLock;

fn setup(demo: Option<bool>) -> (Sim, Value) {
    static WASM: OnceLock<Vec<u8>> = OnceLock::new();
    let wasm = WASM.get_or_init(|| {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../contract/Cargo.toml")
            .canonicalize()
            .unwrap();
        let path = cargo_near_build::build_with_cli(
            cargo_near_build::BuildOpts::builder()
                .manifest_path(manifest.to_str().unwrap())
                .build(),
        )
        .expect("build current finalize source");
        println!("Fresh WASM: {path}");
        std::fs::read(path).unwrap()
    });
    let mut sim = Sim::new();
    for account in [
        "creator.near",
        "worker.near",
        "reviewer.near",
        "finalizer.near",
    ] {
        sim.create_account(account, Balance::from_near(10));
    }
    sim.deploy("anchor.near", wasm.clone());
    sim.call("creator.near", "anchor.near", "new")
        .args_json(demo.map_or_else(|| json!({}), |demo| json!({"demo": demo})))
        .transact()
        .assert_success();
    let verifier: Value = sim
        .view("anchor.near", "get_verifier_info", json!({}))
        .json();
    let verifier_hash = verifier["verifier_hash"].as_str().unwrap().to_owned();
    sim.call("creator.near", "anchor.near", "register")
        .args_json(
            json!({"run_id": "gate1", "task_spec": "eyJ0YXNrX3R5cGUiOiJ0Ml9mZWVfc3BsaXRfdjEiLCJmZWVfYmFzaXNfcG9pbnRzIjoyNTB9", "verifier": {"task_spec_hash": "9f49b7cffedde917676115cd3a4885f451bb5860ac593251c1da8598fc2a1416", "verifier_hash": verifier_hash, "verifier_key": null}, "agents": ["creator.near", "worker.near", "reviewer.near"]}),
        )
        .deposit(Balance::from_millinear(3010))
        .transact()
        .assert_success();
    let mut head = String::new();
    let mut hashes = Vec::new();
    // Synthetic simulator fixture, not a verified T2 failure or testnet run.
    let payloads = ["ab".repeat(32), "cd".repeat(32), "ef".repeat(32)];
    for (seq, (actor, kind)) in [
        ("creator.near", "task_assigned"),
        ("worker.near", "settlement"),
        ("reviewer.near", "settlement_check_failed"),
    ]
    .into_iter()
    .enumerate()
    {
        let out = sim
            .call(actor, "anchor.near", "anchor")
            .args_json(json!({"run_id": "gate1", "seq": seq, "actor": actor,
                "kind": kind, "task": if seq < 2 { json!({"assignment_seq": 0, "worker": "worker.near", "parent_assignment_seq": null}) } else { Value::Null }, "payload_hash": payloads[seq], "prev_hash": head}))
            .deposit(Balance::from_millinear(10))
            .transact();
        out.assert_success();
        head = out.json();
        hashes.push(out.hash.to_string());
    }
    (
        sim,
        json!({"run_id": "gate1", "blamed": "worker.near", "failure_class": "F1", "verifier_hash": verifier_hash,
        "evidence_seqs": [0, 1, 2], "evidence_payload_hashes": payloads, "evidence_tx_hashes": hashes}),
    )
}

fn file(sim: &mut Sim, args: &Value) -> u64 {
    let out = sim
        .call("creator.near", "anchor.near", "file_claim")
        .args_json(args.clone())
        .deposit(Balance::from_millinear(260))
        .transact();
    out.assert_success();
    out.json()
}

fn claim(sim: &Sim, id: u64) -> Value {
    sim.view("anchor.near", "get_claim", json!({"claim_id": id}))
        .json()
}

fn run(sim: &Sim, id: &str) -> Value {
    sim.view("anchor.near", "get_run", json!({"run_id": id}))
        .json()
}

fn reputation(sim: &Sim) -> u8 {
    sim.view(
        "anchor.near",
        "get_reputation",
        json!({"agent": "worker.near"}),
    )
    .json()
}

#[test]
fn demo_boundary_payout_and_idempotency() {
    let (mut sim, args) = setup(Some(true));
    let id = file(&mut sim, &args);
    let pending = claim(&sim, id);
    let deadline = pending["challenge_deadline"].as_u64().unwrap();
    assert_eq!(deadline - pending["claim_height"].as_u64().unwrap(), 60);
    assert_eq!(reputation(&sim), 100);
    sim.call("finalizer.near", "anchor.near", "finalize")
        .args_json(json!({"claim_id": 999}))
        .transact()
        .assert_panic_contains("unknown claim");
    let early = sim
        .call("finalizer.near", "anchor.near", "finalize")
        .args_json(json!({"claim_id": id}))
        .transact();
    early.assert_panic_contains("challenge window still open");
    assert_eq!(claim(&sim, id), pending);
    assert_eq!(early.effects().near_net("anchor.near"), Delta::ZERO);
    let run_before = run(&sim, "gate1");
    let reporter_before = sim.balance("creator.near").as_yoctonear();
    let worker_before = sim.balance("worker.near");

    // Queue calls one block apart: the first executes exactly at H+60, the
    // second at H+61. The simulator's cross-account receipt delay is 2 blocks.
    sim.advance_to(deadline - 2);
    let at_deadline = sim
        .call("finalizer.near", "anchor.near", "finalize")
        .args_json(json!({"claim_id": id}))
        .submit();
    sim.step();
    let after_deadline = sim
        .call("reviewer.near", "anchor.near", "finalize")
        .args_json(json!({"claim_id": id}))
        .submit();
    sim.settle();
    let boundary = sim.outcome(at_deadline);
    boundary.assert_panic_contains("challenge window still open");
    assert_eq!(boundary.receipt("finalize").unwrap().block_height, deadline);
    let finalized = sim.outcome(after_deadline);
    finalized.assert_success();
    assert_eq!(
        finalized.receipt("finalize").unwrap().block_height,
        deadline + 1
    );
    let payout = Balance::from_millinear(450).as_yoctonear();
    let effects = finalized.effects();
    assert_eq!(
        effects
            .near_paid("anchor.near", "creator.near")
            .as_yoctonear(),
        payout
    );
    assert_eq!(effects.near_net("anchor.near"), Delta::loss(payout));
    assert_eq!(effects.near_net("creator.near"), Delta::gain(payout));
    assert_eq!(effects.storage_bytes("anchor.near"), 0);
    assert!(effects.pending.is_empty());
    assert_eq!(
        sim.balance("creator.near").as_yoctonear(),
        reporter_before + payout
    );
    assert_eq!(sim.balance("worker.near"), worker_before);
    assert_eq!(reputation(&sim), 90);
    let final_claim = claim(&sim, id);
    assert_eq!(final_claim["status"], "Finalized");
    assert_eq!(final_claim["bond"], "0");
    let run_after = run(&sim, "gate1");
    assert_eq!(
        run_after["agents"][1]["locked_stake"],
        Balance::from_millinear(750).as_yoctonear().to_string()
    );
    assert_eq!(run_after["agents"][0], run_before["agents"][0]);
    assert_eq!(run_after["agents"][2], run_before["agents"][2]);
    assert_eq!(run_after["events"], run_before["events"]);
    assert_eq!(run_after["head_hash"], run_before["head_hash"]);
    let treasury: String = sim.view("anchor.near", "get_treasury", json!({})).json();
    assert_eq!(
        treasury,
        Balance::from_millinear(50).as_yoctonear().to_string()
    );
    // Initial 3.25 NEAR liabilities become 2.75 stakes + 0.05 treasury,
    // with 0.45 transferred: 3.25 = 2.75 + 0.05 + 0.45, storage excluded.
    let liabilities: u128 = run_after["agents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|agent| {
            agent["locked_stake"]
                .as_str()
                .unwrap()
                .parse::<u128>()
                .unwrap()
        })
        .sum();
    assert_eq!(
        Balance::from_millinear(3250).as_yoctonear(),
        liabilities + treasury.parse::<u128>().unwrap() + payout
    );
    for caller in ["finalizer.near", "reviewer.near"] {
        let repeated = sim
            .call(caller, "anchor.near", "finalize")
            .args_json(json!({"claim_id": id}))
            .transact();
        repeated.assert_success();
        assert!(repeated.effects().near.is_empty());
        assert_eq!(repeated.effects().storage_bytes("anchor.near"), 0);
        assert_eq!(claim(&sim, id), final_claim);
        assert_eq!(run(&sim, "gate1"), run_after);
        assert_eq!(reputation(&sim), 90);
        assert_eq!(
            sim.view("anchor.near", "get_treasury", json!({}))
                .json::<String>(),
            treasury
        );
    }
    sim.findings().assert_none();
}

#[test]
fn production_default_and_no_cross_run_collateral() {
    let (mut sim, args) = setup(None);
    let ids: Vec<_> = (0..5).map(|_| file(&mut sim, &args)).collect();
    let last = claim(&sim, ids[4]);
    assert_eq!(
        last["challenge_deadline"].as_u64().unwrap() - last["claim_height"].as_u64().unwrap(),
        600
    );
    sim.advance_to(last["challenge_deadline"].as_u64().unwrap());
    for (index, id) in ids[..4].iter().enumerate() {
        sim.call("finalizer.near", "anchor.near", "finalize")
            .args_json(json!({"claim_id": id}))
            .transact()
            .assert_success();
        assert_eq!(reputation(&sim), 100 - 10 * (index as u8 + 1));
    }
    assert_eq!(run(&sim, "gate1")["agents"][1]["locked_stake"], "0");
    let verifier_hash = run(&sim, "gate1")["verifier"]["verifier_hash"]
        .as_str()
        .unwrap()
        .to_owned();
    sim.call("creator.near", "anchor.near", "register")
        .args_json(json!({"run_id": "second", "task_spec": "eyJ0YXNrX3R5cGUiOiJ0Ml9mZWVfc3BsaXRfdjEiLCJmZWVfYmFzaXNfcG9pbnRzIjoyNTB9", "verifier": {"task_spec_hash": "9f49b7cffedde917676115cd3a4885f451bb5860ac593251c1da8598fc2a1416", "verifier_hash": verifier_hash, "verifier_key": null}, "agents": ["worker.near"]}))
        .deposit(Balance::from_millinear(1010))
        .transact()
        .assert_success();
    assert_eq!(reputation(&sim), 60); // registration must preserve existing scores
    let finalized = sim
        .call("finalizer.near", "anchor.near", "finalize")
        .args_json(json!({"claim_id": ids[4]}))
        .transact();
    finalized.assert_success();
    assert_eq!(
        finalized.effects().near_paid("anchor.near", "creator.near"),
        Balance::from_millinear(250)
    );
    assert_eq!(finalized.effects().storage_bytes("anchor.near"), 0);
    assert_eq!(claim(&sim, ids[4])["status"], "Finalized");
    assert_eq!(reputation(&sim), 50);
    assert_eq!(
        run(&sim, "second")["agents"][0]["locked_stake"],
        Balance::from_near(1).as_yoctonear().to_string()
    );
    assert_eq!(
        sim.view("anchor.near", "get_treasury", json!({}))
            .json::<String>(),
        Balance::from_millinear(200).as_yoctonear().to_string()
    );
    sim.findings().assert_none();
}
