//! Exercise the CLI's actual command engine against the pinned NEAR simulator.
use anyhow::{Result, ensure};
use clap::Parser;
use near_contract_sim::{Balance, Gas, Sim};
use serde_json::{Value, json};
use std::{
    sync::{
        OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};
use wbts_cli::{App, Command, Engine, NEAR, Transport, Tx};

const CONTRACT: &str = "engine.testnet";
const REPORTER: &str = "coord.testnet";
const WORKER: &str = "worker.testnet";
const CHALLENGER: &str = "watcher.testnet";

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
        .expect("build unchanged contract source");
        std::fs::read(path).unwrap()
    })
}

struct Simulator(Sim);
impl Transport for Simulator {
    fn view(&mut self, method: &str, args: Value) -> Result<Value> {
        Ok(self.0.view(CONTRACT, method, args).json())
    }
    fn check_signers(&mut self, _: &[String]) -> Result<()> {
        Ok(())
    }
    fn wait_after(&mut self, deadline: u64) -> Result<()> {
        self.0.advance_to(deadline + 1);
        Ok(())
    }
    fn snapshot(&mut self, accounts: &[String]) -> Result<Value> {
        let mut balances = serde_json::Map::new();
        let mut reps = serde_json::Map::new();
        for id in accounts {
            balances.insert(
                id.clone(),
                json!({"amount":self.0.balance(id.as_str()).as_yoctonear().to_string()}),
            );
            reps.insert(
                id.clone(),
                self.view("get_reputation", json!({"agent":id}))?,
            );
        }
        Ok(
            json!({"accounts":balances,"reputations":reps,"treasury":self.view("get_treasury",json!({}))?}),
        )
    }
    fn call(
        &mut self,
        signer: &str,
        method: &str,
        args: Value,
        principal: u128,
        allowance: u128,
    ) -> Result<Tx> {
        let start = Instant::now();
        let out = self
            .0
            .call(signer, CONTRACT, method)
            .args_json(args)
            .gas(Gas::from_teragas(100))
            .deposit(Balance::from_yoctonear(principal + allowance))
            .transact();
        ensure!(
            out.is_success(),
            "simulator {method} failed: {:?}",
            out.failure()
        );
        for receipt in &out.receipts {
            ensure!(receipt.failure().is_none(), "failed detached receipt");
        }
        let bytes = u64::try_from(out.effects().storage_bytes(CONTRACT))?;
        let storage = self.0.storage_cost(bytes).as_yoctonear();
        let result = if out.bytes().is_empty() {
            Value::Null
        } else {
            out.json()
        };
        Ok(Tx {
            hash: out.hash.to_string(),
            signer: signer.into(),
            method: method.into(),
            block_height: out.receipt(method).unwrap().block_height,
            elapsed_ms: start.elapsed().as_millis(),
            started_unix_ms: 0,
            ended_unix_ms: start.elapsed().as_millis(),
            gas_burnt: out.gas_burnt().as_gas(),
            gas_yocto: out.effects().gas_paid().as_yoctonear().to_string(),
            storage_bytes: bytes,
            storage_yocto: storage.to_string(),
            attached_yocto: (principal + allowance).to_string(),
            excess_refunded_yocto: (allowance - storage).to_string(),
            result,
            outcome: serde_json::to_value(&out)?,
        })
    }
}

fn setup() -> (Engine<Simulator>, App) {
    let mut sim = Sim::new();
    for id in [REPORTER, WORKER, CHALLENGER] {
        sim.create_account(id, Balance::from_near(100));
    }
    sim.deploy(CONTRACT, wasm().clone());
    sim.call(REPORTER, CONTRACT, "new")
        .args_json(json!({"demo":false}))
        .transact()
        .assert_success();
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "wbts-cli-test-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let app = App::parse_from([
        "cli",
        "--contract",
        CONTRACT,
        "--journal-dir",
        dir.to_str().unwrap(),
        "run-demo",
        "s2",
    ]);
    (Engine::new(Simulator(sim), &app).unwrap(), app)
}

fn register(engine: &mut Engine<Simulator>) {
    engine
        .execute(Command::Register {
            run_id: Some("commands".into()),
            signer: REPORTER.into(),
            agents: vec![REPORTER.into(), WORKER.into(), CHALLENGER.into()],
        })
        .unwrap();
}
fn anchor(engine: &mut Engine<Simulator>, dir: &std::path::Path, three: bool) {
    for (seq, signer, kind, body, worker, assignment) in [
        (
            0,
            REPORTER,
            "task_assigned",
            wbts_cli::INPUT,
            Some(WORKER.into()),
            Some(0),
        ),
        (
            1,
            WORKER,
            "settlement",
            wbts_cli::OUTPUT,
            Some(WORKER.into()),
            Some(0),
        ),
        (2, CHALLENGER, "review", b"{}".as_slice(), None, None),
    ] {
        if seq == 2 && !three {
            break;
        }
        let payload = dir.join(format!("payload-{seq}.json"));
        std::fs::write(&payload, body).unwrap();
        engine
            .execute(Command::Anchor {
                run_id: "commands".into(),
                signer: signer.into(),
                kind: kind.into(),
                payload,
                worker,
                assignment_seq: assignment,
            })
            .unwrap();
    }
}
fn file(engine: &mut Engine<Simulator>) {
    engine
        .execute(Command::FileClaim {
            run_id: "commands".into(),
            signer: REPORTER.into(),
            blamed: WORKER.into(),
            failure_class: "F1".into(),
            evidence_seqs: vec![0, 1],
        })
        .unwrap();
}

#[test]
fn register_command_funds_three_agents() {
    let (mut e, _) = setup();
    register(&mut e);
    let run = e
        .transport
        .view("get_run", json!({"run_id":"commands"}))
        .unwrap();
    assert_eq!(run["agents"].as_array().unwrap().len(), 3);
    for agent in run["agents"].as_array().unwrap() {
        assert_eq!(agent["locked_stake"], NEAR.to_string());
    }
}

#[test]
fn anchor_command_three_transactions_and_head_mismatch_stops() {
    let (mut e, app) = setup();
    register(&mut e);
    anchor(&mut e, &app.journal_dir, true);
    let run = e
        .transport
        .view("get_run", json!({"run_id":"commands"}))
        .unwrap();
    assert_eq!(run["events"].as_array().unwrap().len(), 3);
    assert_eq!(
        e.journal
            .transactions
            .iter()
            .filter(|t| t.method == "anchor")
            .count(),
        3
    );
    assert_eq!(run["head_hash"], e.journal.head);
    e.journal.head = "00".repeat(32);
    let n = e.journal.transactions.len();
    assert!(e.anchor(CHALLENGER, "review", b"{}", Value::Null).is_err());
    assert_eq!(e.journal.transactions.len(), n);
}

#[test]
fn file_claim_command_records_pending_bond_and_real_evidence() {
    let (mut e, app) = setup();
    register(&mut e);
    anchor(&mut e, &app.journal_dir, false);
    file(&mut e);
    let claim = e
        .transport
        .view("get_claim", json!({"claim_id":0}))
        .unwrap();
    assert_eq!(claim["status"], "Pending");
    assert_eq!(claim["bond"], (NEAR / 4).to_string());
    assert_eq!(
        claim["evidence_tx_hashes"][0],
        e.journal.events[0]["tx_hash"]
    );
}

#[test]
fn challenge_command_funds_result_storage() {
    let (mut e, app) = setup();
    register(&mut e);
    anchor(&mut e, &app.journal_dir, false);
    file(&mut e);
    e.execute(Command::Challenge {
        run_id: "commands".into(),
        signer: CHALLENGER.into(),
        claim_id: 0,
    })
    .unwrap();
    let claim = e
        .transport
        .view("get_claim", json!({"claim_id":0}))
        .unwrap();
    assert_eq!(claim["status"], "Challenged");
    assert_eq!(claim["challenge_bond"], (NEAR / 4).to_string());
    assert!(e.journal.transactions.last().unwrap().storage_bytes > 0);
}

#[test]
fn run_demo_s2_watcher_sponsors_exact_money_and_fresh_reputation_then_repeat() {
    let (mut e, _) = setup();
    for (run, expected) in [("s2-first", [85, 102, 105]), ("s2-repeat", [70, 104, 110])] {
        e.execute(Command::RunDemo {
            scenario: "s2".into(),
            run_id: Some(run.into()),
            reporter: REPORTER.into(),
            worker: WORKER.into(),
            challenger: CHALLENGER.into(),
            registrar: Some(CHALLENGER.into()),
        })
        .unwrap();
        let proof = e.journal.proof.as_ref().unwrap();
        assert_eq!(proof["run"]["creator"], CHALLENGER);
        for (i, id) in [REPORTER, WORKER, CHALLENGER].iter().enumerate() {
            assert_eq!(proof["after"]["reputations"][*id], expected[i]);
        }
        assert_eq!(
            proof["verification"]["payout_yocto"],
            (NEAR * 45 / 100).to_string()
        );
        assert_eq!(
            proof["verification"]["treasury_delta_yocto"],
            (NEAR / 20).to_string()
        );
        assert_eq!(e.journal.transactions.len(), 7);
        assert_eq!(e.journal.preview.as_ref().unwrap()["verdict"], "Overturned");
        assert_eq!(proof["claim"]["status"], "Finalized");
        assert_eq!(proof["run"]["agents"][1]["locked_stake"], NEAR.to_string());
    }
}

#[test]
fn failed_registration_stops_s2_before_any_anchor_or_bond() {
    let (mut engine, mut app) = setup();
    app.storage_allowance = 1;
    // Keep the initialized simulator; only the CLI's deposit allowance changes.
    engine = Engine::new(engine.transport, &app).unwrap();
    let result = engine.execute(Command::RunDemo {
        scenario: "s2".into(),
        run_id: Some("too-small".into()),
        reporter: REPORTER.into(),
        worker: WORKER.into(),
        challenger: CHALLENGER.into(),
        registrar: None,
    });
    assert!(result.is_err());
    assert!(
        engine
            .journal
            .failure
            .as_ref()
            .unwrap()
            .contains("register failed")
    );
    assert!(engine.journal.transactions.is_empty());
    assert!(
        engine
            .transport
            .view("get_run", json!({"run_id":"too-small"}))
            .unwrap()
            .is_null()
    );
    assert!(
        engine
            .transport
            .view("get_claim", json!({"claim_id":0}))
            .unwrap()
            .is_null()
    );
}

fn demo(scenario: &str, self_challenge: bool) -> Engine<Simulator> {
    let (mut e, _) = setup();
    // The public worker alias selects the configured worker and standard watcher.
    // Other tests keep their existing account IDs and S2 fixtures.
    e.transport
        .0
        .create_account("watcher-wbts.testnet", Balance::from_near(100));
    e.execute(Command::RunDemo {
        scenario: scenario.into(),
        run_id: Some(format!("harness-{scenario}")),
        reporter: REPORTER.into(),
        worker: WORKER.into(),
        challenger: if self_challenge {
            "worker".into()
        } else {
            CHALLENGER.into()
        },
        registrar: Some(WORKER.into()),
    })
    .unwrap();
    e
}

#[test]
fn s1_true_blame() {
    let e = demo("s1", false);
    let p = e.journal.proof.unwrap();
    assert_eq!(p["run"]["creator"], WORKER);
    assert_eq!(p["after"]["reputations"][WORKER], 90);
    assert_eq!(
        p["verification"]["payout_yocto"],
        (NEAR * 45 / 100).to_string()
    );
    assert_eq!(e.journal.transactions.len(), 5);
    assert!(e.journal.preview.is_none());
}

#[test]
fn s5_precondition() {
    let e = demo("s5", false);
    let p = e.journal.proof.unwrap();
    assert_eq!(
        p["verification"]["payout_yocto"],
        (NEAR * 65 / 100).to_string()
    );
    assert_eq!(p["after"]["reputations"][REPORTER], 90);
    assert_eq!(p["after"]["reputations"][WORKER], 100);
    assert_eq!(e.journal.preview.unwrap()["verdict"], "Upheld");
}

#[test]
fn s6_causal_order_both_challengers() {
    for self_challenge in [false, true] {
        let e = demo("s6", self_challenge);
        let p = e.journal.proof.unwrap();
        assert_eq!(p["after"]["reputations"][REPORTER], 85);
        assert_eq!(
            p["after"]["reputations"][WORKER],
            if self_challenge { 107 } else { 102 }
        );
        assert_eq!(p["claim"]["corrected_attribution"], REPORTER);
        assert_eq!(
            p["claim"]["adjudication_result"]["finding"]["failure_class"],
            "F3"
        );
        assert_eq!(p["verification"]["stake_slash_yocto"], "0");
        assert_eq!(
            p["verification"]["payout_yocto"],
            (NEAR * 45 / 100).to_string()
        );
    }
}

#[test]
fn s3_tamper_diagnostic_no_claim_or_money() {
    let mut e = demo("s3", false);
    assert_eq!(e.journal.transactions.len(), 3);
    assert_eq!(e.journal.proof.as_ref().unwrap()["mismatch"]["seq"], 1);
    assert_eq!(
        e.journal.proof.as_ref().unwrap()["mismatch"]["signer"],
        WORKER
    );
    e.execute(Command::VerifyRun {
        run_id: "harness-s3".into(),
        journal: None,
    })
    .unwrap();
    // Also catch deletion/reordering and a locally substituted head.
    let chain = e
        .transport
        .view("get_run", json!({"run_id":"harness-s3"}))
        .unwrap();
    e.journal.events.swap(0, 1);
    assert_eq!(
        wbts_cli::verify_journal(&e.journal, &chain)
            .unwrap()
            .unwrap()
            .seq,
        0
    );
    e.journal.events.swap(0, 1);
    e.journal.head = "00".repeat(32);
    assert_eq!(
        wbts_cli::verify_journal(&e.journal, &chain)
            .unwrap()
            .unwrap()
            .seq,
        2
    );
}

#[test]
fn funding_gate_stops_before_registration() {
    for (millinear, scenario) in [(100, "s1"), (3200, "s2")] {
        let (mut e, _) = setup();
        e.transport
            .0
            .create_account("poor.testnet", Balance::from_millinear(millinear));
        let err = e
            .execute(Command::RunDemo {
                scenario: scenario.into(),
                run_id: Some("unfunded".into()),
                reporter: if scenario == "s2" {
                    "poor.testnet".into()
                } else {
                    REPORTER.into()
                },
                worker: WORKER.into(),
                challenger: CHALLENGER.into(),
                registrar: Some("poor.testnet".into()),
            })
            .unwrap_err();
        assert!(err.to_string().contains("funding gate"));
        assert!(e.journal.transactions.is_empty());
        assert!(
            e.transport
                .view("get_run", json!({"run_id":"unfunded"}))
                .unwrap()
                .is_null()
        );
    }
}
