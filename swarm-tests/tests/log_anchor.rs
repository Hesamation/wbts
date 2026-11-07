use near_contract_sim::{Balance, Delta, Sim};
use serde_json::{Value, json};

/// Build the current source, then exercise the WASM in the nearcore runtime.
#[test]
fn registered_run_checks_and_storage_rollback() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../contract/Cargo.toml")
        .canonicalize()
        .unwrap();
    let wasm = cargo_near_build::build_with_cli(
        cargo_near_build::BuildOpts::builder()
            .manifest_path(manifest.to_str().unwrap())
            .build(),
    )
    .expect("build current log-anchor source");
    println!("Fresh WASM: {wasm}");

    let mut sim = Sim::new();
    sim.create_account("alice.near", Balance::from_near(10));
    sim.create_account("bob.near", Balance::from_near(10));
    sim.create_account("carol.near", Balance::from_near(10));
    sim.create_account("outsider.near", Balance::from_near(10));
    sim.deploy("anchor.near", std::fs::read(wasm).unwrap());
    sim.call("alice.near", "anchor.near", "new")
        .args_json(json!({}))
        .transact()
        .assert_success();
    let registration =
        json!({"run_id": "gate1", "agents": ["alice.near", "bob.near", "carol.near"]});
    for (args, message) in [
        (
            json!({"run_id": "", "agents": ["alice.near"]}),
            "run_id must not be empty",
        ),
        (
            json!({"run_id": "empty", "agents": []}),
            "agents must not be empty",
        ),
        (
            json!({"run_id": "duplicate", "agents": ["alice.near", "alice.near"]}),
            "agents must be unique",
        ),
    ] {
        sim.call("alice.near", "anchor.near", "register")
            .args_json(args)
            .deposit(Balance::from_near(4))
            .transact()
            .assert_panic_contains(message);
    }
    // Principal alone does not pay for storage. Rejection must remove all state
    // written during registration, permitting a fully funded retry of this ID.
    sim.call("alice.near", "anchor.near", "register")
        .args_json(registration.clone())
        .deposit(Balance::from_near(3))
        .transact()
        .assert_panic_contains("insufficient stake and storage deposit");
    assert_eq!(
        sim.view("anchor.near", "get_run", json!({"run_id": "gate1"}))
            .json::<Value>(),
        Value::Null
    );
    let registered = sim
        .call("alice.near", "anchor.near", "register")
        .args_json(registration.clone())
        .deposit(Balance::from_millinear(3010))
        .transact();
    registered.assert_success();
    let effects = registered.effects();
    let storage_bytes = u64::try_from(effects.storage_bytes("anchor.near")).unwrap();
    let storage_cost = sim.storage_cost(storage_bytes).as_yoctonear();
    let principal = Balance::from_near(3).as_yoctonear();
    assert_eq!(
        effects.near_net("anchor.near"),
        Delta::gain(principal + storage_cost)
    );
    assert_eq!(
        effects
            .near_paid("anchor.near", "alice.near")
            .as_yoctonear(),
        Balance::from_millinear(3010).as_yoctonear() - principal - storage_cost
    );
    assert!(effects.pending.is_empty());
    let run_before: Value = sim
        .view("anchor.near", "get_run", json!({"run_id": "gate1"}))
        .json();
    assert_eq!(run_before["creator"], "alice.near");
    assert_eq!(run_before["agents"].as_array().unwrap().len(), 3);
    let one_near = Balance::from_near(1).as_yoctonear().to_string();
    for (agent, account) in run_before["agents"].as_array().unwrap().iter().zip([
        "alice.near",
        "bob.near",
        "carol.near",
    ]) {
        assert_eq!(agent["account_id"], account);
        assert_eq!(agent["locked_stake"], one_near);
    }
    assert_eq!(run_before["events"], json!([]));
    assert_eq!(run_before["head_hash"], "");
    sim.call("bob.near", "anchor.near", "register")
        .args_json(registration)
        .deposit(Balance::from_near(4))
        .transact()
        .assert_panic_contains("run already registered");
    assert_eq!(
        sim.view("anchor.near", "get_run", json!({"run_id": "gate1"}))
            .json::<Value>(),
        run_before
    );
    let event = |seq: u64, prev: &str| {
        json!({"run_id": "gate1", "seq": seq, "actor": "alice.near",
            "kind": "task_assigned", "payload_hash": "ab".repeat(32), "prev_hash": prev})
    };
    let mut unknown = event(0, "");
    unknown["run_id"] = json!("unregistered");
    sim.call("alice.near", "anchor.near", "anchor")
        .args_json(unknown)
        .deposit(Balance::from_millinear(10))
        .transact()
        .assert_panic_contains("unknown run");
    let mut outsider = event(0, "");
    outsider["actor"] = json!("outsider.near");
    sim.call("outsider.near", "anchor.near", "anchor")
        .args_json(outsider)
        .deposit(Balance::from_millinear(10))
        .transact()
        .assert_panic_contains("actor must be registered agent");
    let first = sim
        .call("alice.near", "anchor.near", "anchor")
        .args_json(event(0, ""))
        .deposit(Balance::from_millinear(10))
        .transact();
    first.assert_success();
    let head: String = first.json();
    assert_eq!(head.len(), 64);
    let stored: Value = sim
        .view(
            "anchor.near",
            "get_event",
            json!({"run_id": "gate1", "seq": 0}),
        )
        .json();
    assert_eq!(stored["actor"], "alice.near");
    assert_eq!(stored["payload_hash"], "ab".repeat(32));

    // Duplicate and skipped sequences must leave the existing chain unchanged.
    for seq in [0, 2] {
        sim.call("alice.near", "anchor.near", "anchor")
            .args_json(event(seq, &head))
            .deposit(Balance::from_millinear(10))
            .transact()
            .assert_panic_contains("seq must extend the log");
    }
    sim.call("alice.near", "anchor.near", "anchor")
        .args_json(event(1, &"00".repeat(32)))
        .deposit(Balance::from_millinear(10))
        .transact()
        .assert_panic_contains("prev_hash must equal head");
    sim.call("bob.near", "anchor.near", "anchor")
        .args_json(event(1, &head))
        .deposit(Balance::from_millinear(10))
        .transact()
        .assert_panic_contains("actor must be caller");
    // Storage writes occur before the deposit assertion; receipt failure must
    // roll them back in the runtime, allowing the same sequence to be retried.
    sim.call("alice.near", "anchor.near", "anchor")
        .args_json(event(1, &head))
        .transact()
        .assert_panic_contains("insufficient storage deposit");
    assert_eq!(
        sim.view("anchor.near", "run_len", json!({"run_id": "gate1"}))
            .json::<u64>(),
        1
    );
    sim.call("alice.near", "anchor.near", "anchor")
        .args_json(event(1, &head))
        .deposit(Balance::from_millinear(10))
        .transact()
        .assert_success();
    assert_eq!(
        sim.view("anchor.near", "run_len", json!({"run_id": "gate1"}))
            .json::<u64>(),
        2
    );
    assert_eq!(
        sim.view("anchor.near", "verify_chain", json!({"run_id": "gate1"}))
            .json::<Option<u64>>(),
        None
    );
    assert_eq!(
        sim.view(
            "anchor.near",
            "get_event",
            json!({"run_id": "gate1", "seq": u64::MAX})
        )
        .json::<Value>(),
        Value::Null
    );
    sim.view("anchor.near", "verify_chain", json!({"run_id": "missing"}))
        .assert_panic_contains("unknown run");
    // A second run has its own head and collateral, rather than sharing state.
    sim.call("bob.near", "anchor.near", "register")
        .args_json(json!({"run_id": "second", "agents": ["bob.near"]}))
        .deposit(Balance::from_millinear(1010))
        .transact()
        .assert_success();
    assert_eq!(
        sim.view("anchor.near", "run_len", json!({"run_id": "second"}))
            .json::<u64>(),
        0
    );
    let mut second = event(0, "");
    second["run_id"] = json!("second");
    second["actor"] = json!("bob.near");
    sim.call("bob.near", "anchor.near", "anchor")
        .args_json(second)
        .deposit(Balance::from_millinear(10))
        .transact()
        .assert_success();
    assert_eq!(
        sim.view("anchor.near", "run_len", json!({"run_id": "gate1"}))
            .json::<u64>(),
        2
    );
    assert_eq!(
        sim.view("anchor.near", "run_len", json!({"run_id": "second"}))
            .json::<u64>(),
        1
    );
    sim.findings().assert_none();
}
