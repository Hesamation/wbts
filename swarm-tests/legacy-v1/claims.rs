use near_contract_sim::{Balance, Delta, Sim};
use serde_json::{Value, json};

#[test]
fn pending_claim_bond_evidence_and_rollback() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../contract/Cargo.toml")
        .canonicalize()
        .unwrap();
    let wasm = cargo_near_build::build_with_cli(
        cargo_near_build::BuildOpts::builder()
            .manifest_path(manifest.to_str().unwrap())
            .build(),
    )
    .expect("build current claim source");
    println!("Fresh WASM: {wasm}");
    let mut sim = Sim::new();
    for account in ["alice.near", "bob.near", "carol.near", "watcher.near"] {
        sim.create_account(account, Balance::from_near(10));
    }
    sim.deploy("anchor.near", std::fs::read(wasm).unwrap());
    sim.call("alice.near", "anchor.near", "new")
        .args_json(json!({}))
        .transact()
        .assert_success();
    let verifier: Value = sim
        .view("anchor.near", "get_verifier_info", json!({}))
        .json();
    let verifier_hash = verifier["verifier_hash"].as_str().unwrap().to_owned();
    sim.call("alice.near", "anchor.near", "register")
        .args_json(json!({"run_id": "gate1", "task_spec": "eyJ0YXNrX3R5cGUiOiJ0Ml9mZWVfc3BsaXRfdjEiLCJmZWVfYmFzaXNfcG9pbnRzIjoyNTB9", "verifier": {"task_spec_hash": "9f49b7cffedde917676115cd3a4885f451bb5860ac593251c1da8598fc2a1416", "verifier_hash": verifier_hash, "verifier_key": null}, "agents": ["alice.near", "bob.near", "carol.near"]}))
        .deposit(Balance::from_millinear(3010))
        .transact()
        .assert_success();

    let payload_hashes = ["ab".repeat(32), "cd".repeat(32), "ef".repeat(32)];
    let mut head = String::new();
    let mut tx_hashes = Vec::new();
    for (seq, (actor, kind)) in [
        ("alice.near", "task_assigned"),
        ("bob.near", "settlement"),
        ("carol.near", "settlement_check_failed"),
    ]
    .into_iter()
    .enumerate()
    {
        let anchored = sim
            .call(actor, "anchor.near", "anchor")
            .args_json(json!({"run_id": "gate1", "seq": seq, "actor": actor,
                "kind": kind, "task": if seq < 2 { json!({"assignment_seq": 0, "worker": "bob.near", "parent_assignment_seq": null}) } else { Value::Null }, "payload_hash": payload_hashes[seq], "prev_hash": head}))
            .deposit(Balance::from_millinear(10))
            .transact();
        anchored.assert_success();
        head = anchored.json();
        // These are actual simulator transaction hashes, not testnet evidence.
        tx_hashes.push(anchored.hash.to_string());
    }
    let args = json!({"run_id": "gate1", "blamed": "bob.near", "failure_class": "F1", "verifier_hash": verifier_hash,
        "evidence_seqs": [0, 1, 2], "evidence_payload_hashes": payload_hashes,
        "evidence_tx_hashes": tx_hashes});
    let run_before: Value = sim
        .view("anchor.near", "get_run", json!({"run_id": "gate1"}))
        .json();
    for (field, value, message) in [
        ("run_id", json!("missing"), "unknown run"),
        (
            "verifier_hash",
            json!("ff".repeat(32)),
            "verifier hash mismatch",
        ),
        (
            "blamed",
            json!("watcher.near"),
            "blamed must be registered agent",
        ),
        ("evidence_seqs", json!([]), "evidence must not be empty"),
        (
            "evidence_payload_hashes",
            json!([]),
            "evidence lengths must match",
        ),
        (
            "evidence_tx_hashes",
            json!([]),
            "evidence lengths must match",
        ),
        (
            "evidence_seqs",
            json!([0, 0, 2]),
            "evidence sequences must be unique and increasing",
        ),
        (
            "evidence_seqs",
            json!([1, 0, 2]),
            "evidence sequences must be unique and increasing",
        ),
        (
            "evidence_seqs",
            json!([0, 1, 99]),
            "evidence event does not exist",
        ),
        (
            "evidence_payload_hashes",
            json!(["00".repeat(32), payload_hashes[1], payload_hashes[2]]),
            "evidence payload hash mismatch",
        ),
        (
            "evidence_tx_hashes",
            json!(["not-a-hash!", tx_hashes[1], tx_hashes[2]]),
            "invalid evidence transaction hash",
        ),
        ("failure_class", json!("F6"), "unknown variant"),
    ] {
        let mut invalid = args.clone();
        invalid[field] = value;
        let rejected = sim
            .call("alice.near", "anchor.near", "file_claim")
            .args_json(invalid)
            .deposit(Balance::from_millinear(260))
            .transact();
        rejected.assert_panic_contains(message);
        // All principal returns on rejection; gas is charged separately.
        assert_eq!(rejected.effects().near_net("anchor.near"), Delta::ZERO);
        assert_eq!(
            sim.view("anchor.near", "get_claim", json!({"claim_id": 0}))
                .json::<Value>(),
            Value::Null
        );
    }
    for deposit in [
        Balance::ZERO,
        Balance::from_millinear(249),
        Balance::from_millinear(250),
    ] {
        let rejected = sim
            .call("alice.near", "anchor.near", "file_claim")
            .args_json(args.clone())
            .deposit(deposit)
            .transact();
        rejected.assert_panic_contains("insufficient bond and storage deposit");
        assert_eq!(rejected.effects().near_net("anchor.near"), Delta::ZERO);
        assert_eq!(
            sim.view("anchor.near", "get_claim", json!({"claim_id": 0}))
                .json::<Value>(),
            Value::Null
        );
    }
    let filed = sim
        .call("alice.near", "anchor.near", "file_claim")
        .args_json(args.clone())
        .deposit(Balance::from_millinear(260))
        .transact();
    filed.assert_success();
    assert_eq!(filed.json::<u64>(), 0);
    let claim: Value = sim
        .view("anchor.near", "get_claim", json!({"claim_id": 0}))
        .json();
    assert_eq!(claim["claim_id"], 0);
    assert_eq!(claim["run_id"], "gate1");
    assert_eq!(claim["reporter"], "alice.near");
    assert_eq!(claim["blamed"], "bob.near");
    assert_eq!(claim["failure_class"], "F1");
    assert_eq!(claim["evidence_seqs"], args["evidence_seqs"]);
    assert_eq!(
        claim["evidence_payload_hashes"],
        args["evidence_payload_hashes"]
    );
    assert_eq!(claim["evidence_tx_hashes"], args["evidence_tx_hashes"]);
    assert_eq!(claim["status"], "Pending");
    assert_eq!(
        claim["bond"],
        Balance::from_millinear(250).as_yoctonear().to_string()
    );
    assert_eq!(
        claim["claim_height"],
        filed.receipt("file_claim").unwrap().block_height
    );
    assert_eq!(claim["challenger"], Value::Null);
    assert_eq!(claim["corrected_attribution"], Value::Null);
    let effects = filed.effects();
    let storage_cost = sim
        .storage_cost(u64::try_from(effects.storage_bytes("anchor.near")).unwrap())
        .as_yoctonear();
    let bond = Balance::from_millinear(250).as_yoctonear();
    assert_eq!(
        effects.near_net("anchor.near"),
        Delta::gain(bond + storage_cost)
    );
    assert_eq!(
        effects
            .near_paid("anchor.near", "alice.near")
            .as_yoctonear(),
        Balance::from_millinear(260).as_yoctonear() - bond - storage_cost
    );
    assert!(effects.pending.is_empty());
    assert_eq!(
        sim.view("anchor.near", "get_run", json!({"run_id": "gate1"}))
            .json::<Value>(),
        run_before
    );
    // An external watcher can file a separately bonded claim; IDs are not reused.
    let second = sim
        .call("watcher.near", "anchor.near", "file_claim")
        .args_json(args)
        .deposit(Balance::from_millinear(260))
        .transact();
    second.assert_success();
    assert_eq!(second.json::<u64>(), 1);
    assert_eq!(
        sim.view("anchor.near", "get_claim", json!({"claim_id": 1}))
            .json::<Value>()["reporter"],
        "watcher.near"
    );
    assert_eq!(
        sim.view("anchor.near", "get_claim", json!({"claim_id": 0}))
            .json::<Value>(),
        claim
    );
    let pin: Value = sim
        .view("anchor.near", "get_run", json!({"run_id": "gate1"}))
        .json();
    assert_eq!(pin["verifier"]["verifier_hash"], verifier_hash);
    assert_eq!(
        pin["verifier"]["task_spec_hash"],
        "9f49b7cffedde917676115cd3a4885f451bb5860ac593251c1da8598fc2a1416"
    );
    let params = json!({"stake": Balance::from_near(2).as_yoctonear().to_string(),
        "blame_bond": Balance::from_millinear(500).as_yoctonear().to_string(),
        "challenge_bond": Balance::from_millinear(500).as_yoctonear().to_string()});
    sim.call("bob.near", "anchor.near", "update_parameters")
        .args_json(params.clone())
        .transact()
        .assert_panic_contains("owner only");
    let mut invalid = params.clone();
    invalid["challenge_bond"] = json!("1");
    sim.call("alice.near", "anchor.near", "update_parameters")
        .args_json(invalid)
        .transact()
        .assert_panic_contains("bonds must be positive and matching");
    sim.call("alice.near", "anchor.near", "update_parameters")
        .args_json(params.clone())
        .transact()
        .assert_success();
    assert_eq!(
        sim.view("anchor.near", "get_parameters", json!({}))
            .json::<Value>(),
        params
    );
    // Existing claim keeps its original 0.25 matching bond despite the update.
    let rejected = sim
        .call("watcher.near", "anchor.near", "challenge")
        .args_json(json!({"claim_id": 0}))
        .deposit(Balance::from_millinear(250))
        .transact();
    rejected.assert_panic_contains("insufficient challenge bond and storage deposit");
    assert_eq!(rejected.effects().near_net("anchor.near"), Delta::ZERO);
    assert_eq!(
        sim.view("anchor.near", "get_claim", json!({"claim_id": 0}))
            .json::<Value>(),
        claim
    );
    let challenged = sim
        .call("watcher.near", "anchor.near", "challenge")
        .args_json(json!({"claim_id": 0}))
        .deposit(Balance::from_millinear(260))
        .transact();
    challenged.assert_success();
    let challenge_effects = challenged.effects();
    let challenge_storage = sim
        .storage_cost(u64::try_from(challenge_effects.storage_bytes("anchor.near")).unwrap())
        .as_yoctonear();
    assert_eq!(
        challenge_effects.near_net("anchor.near"),
        Delta::gain(bond + challenge_storage)
    );
    assert!(challenge_effects.pending.is_empty());
    let challenged_state: Value = sim
        .view("anchor.near", "get_claim", json!({"claim_id": 0}))
        .json();
    assert_eq!(challenged_state["status"], "Challenged");
    assert_eq!(challenged_state["challenger"], "watcher.near");
    assert_eq!(challenged_state["challenge_bond"], bond.to_string());
    sim.call("bob.near", "anchor.near", "challenge")
        .args_json(json!({"claim_id": 0}))
        .deposit(Balance::from_near(1))
        .transact()
        .assert_panic_contains("claim must be pending");
    sim.call("carol.near", "anchor.near", "finalize")
        .args_json(json!({"claim_id": 0}))
        .transact()
        .assert_panic_contains("claim is not ready to finalize");
    let late: Value = sim
        .view("anchor.near", "get_claim", json!({"claim_id": 1}))
        .json();
    sim.advance_to(late["challenge_deadline"].as_u64().unwrap());
    sim.call("bob.near", "anchor.near", "challenge")
        .args_json(json!({"claim_id": 1}))
        .deposit(Balance::from_near(1))
        .transact()
        .assert_panic_contains("challenge window expired");
    let verifier = json!({"task_spec_hash": "9f49b7cffedde917676115cd3a4885f451bb5860ac593251c1da8598fc2a1416", "verifier_hash": verifier_hash, "verifier_key": null});
    sim.call("alice.near", "anchor.near", "register")
        .args_json(json!({"run_id": "updated", "agents": ["bob.near"], "verifier": verifier, "task_spec": "eyJ0YXNrX3R5cGUiOiJ0Ml9mZWVfc3BsaXRfdjEiLCJmZWVfYmFzaXNfcG9pbnRzIjoyNTB9"}))
        .deposit(Balance::from_millinear(2010))
        .transact()
        .assert_success();
    let updated: Value = sim
        .view("anchor.near", "get_run", json!({"run_id": "updated"}))
        .json();
    assert_eq!(
        updated["agents"][0]["locked_stake"],
        Balance::from_near(2).as_yoctonear().to_string()
    );
    assert_eq!(
        sim.view("anchor.near", "get_run", json!({"run_id": "gate1"}))
            .json::<Value>(),
        run_before
    );
    let anchored = sim.call("bob.near", "anchor.near", "anchor")
        .args_json(json!({"run_id": "updated", "seq": 0, "actor": "bob.near", "kind": "task_assigned", "task": {"assignment_seq": 0, "worker": "bob.near", "parent_assignment_seq": null}, "payload_hash": "ab".repeat(32), "prev_hash": ""}))
        .deposit(Balance::from_millinear(10)).transact();
    anchored.assert_success();
    let new_claim = sim.call("alice.near", "anchor.near", "file_claim")
        .args_json(json!({"run_id": "updated", "blamed": "bob.near", "failure_class": "F1", "verifier_hash": verifier_hash,
            "evidence_seqs": [0], "evidence_payload_hashes": ["ab".repeat(32)], "evidence_tx_hashes": [anchored.hash.to_string()]}))
        .deposit(Balance::from_millinear(520)).transact();
    new_claim.assert_success();
    let new_id: u64 = new_claim.json();
    let new_state: Value = sim
        .view("anchor.near", "get_claim", json!({"claim_id": new_id}))
        .json();
    assert_eq!(
        new_state["bond"],
        Balance::from_millinear(500).as_yoctonear().to_string()
    );
    let deadline = new_state["challenge_deadline"].as_u64().unwrap();
    sim.advance_to(deadline - 2);
    let first = sim
        .call("bob.near", "anchor.near", "challenge")
        .args_json(json!({"claim_id": new_id}))
        .deposit(Balance::from_millinear(510))
        .submit();
    let second = sim
        .call("carol.near", "anchor.near", "challenge")
        .args_json(json!({"claim_id": new_id}))
        .deposit(Balance::from_millinear(510))
        .submit();
    sim.settle();
    let first_outcome = sim.outcome(first);
    first_outcome.assert_success();
    assert_eq!(
        first_outcome.receipt("challenge").unwrap().block_height,
        deadline
    );
    sim.outcome(second)
        .assert_panic_contains("claim must be pending");
    let final_state: Value = sim
        .view("anchor.near", "get_claim", json!({"claim_id": new_id}))
        .json();
    assert_eq!(final_state["challenger"], "bob.near");
    assert_eq!(
        final_state["challenge_bond"],
        Balance::from_millinear(500).as_yoctonear().to_string()
    );
    sim.findings().assert_none();
}
