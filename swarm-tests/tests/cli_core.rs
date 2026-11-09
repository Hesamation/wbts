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
        if self.0.height() <= deadline {
            self.0.advance_to(deadline + 1);
        }
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
            json!({"block_height":self.0.height(),"accounts":balances,"reputations":reps,"treasury":self.view("get_treasury",json!({}))?}),
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
        let bytes = u64::try_from(out.effects().storage_bytes(CONTRACT).max(0))?;
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
            excess_refunded_yocto: "0".into(),
            excess_credited_yocto: allowance.saturating_sub(storage).to_string(),
            result,
            outcome: serde_json::to_value(&out)?,
        })
    }
}

fn setup() -> (Engine<Simulator>, App) {
    let mut sim = Sim::new();
    for id in [REPORTER, WORKER, CHALLENGER, "watcher-wbts.testnet"] {
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

#[test]
fn command_engine_funds_each_participant_and_completes_recovery() {
    let (mut e, app) = setup();
    e.execute(Command::Register {
        run_id: Some("commands".into()),
        signer: REPORTER.into(),
        agents: vec![REPORTER.into(), WORKER.into(), CHALLENGER.into()],
    })
    .unwrap();
    let input = app.journal_dir.join("input.json");
    std::fs::write(&input, wbts_cli::INPUT).unwrap();
    let h = e.transport.0.height();
    e.execute(Command::Offer {
        run_id: "commands".into(),
        signer: REPORTER.into(),
        worker: WORKER.into(),
        payload: input,
        accept_deadline: h + 100,
        work_deadline: h + 1000,
    })
    .unwrap();
    e.execute(Command::Accept {
        run_id: "commands".into(),
        signer: WORKER.into(),
        assignment_seq: 0,
    })
    .unwrap();
    let output = app.journal_dir.join("output.json");
    std::fs::write(&output, wbts_cli::OUTPUT).unwrap();
    e.execute(Command::Complete {
        run_id: "commands".into(),
        signer: WORKER.into(),
        assignment_seq: 0,
        payload: output,
    })
    .unwrap();
    e.execute(Command::FileClaim {
        run_id: "commands".into(),
        signer: REPORTER.into(),
        blamed: WORKER.into(),
        failure_class: "F1".into(),
        evidence_seqs: vec![0, 1],
    })
    .unwrap();
    e.execute(Command::Challenge {
        run_id: "commands".into(),
        signer: CHALLENGER.into(),
        claim_id: 0,
    })
    .unwrap();
    let c = e
        .transport
        .view("get_claim", json!({"claim_id":0}))
        .unwrap();
    assert_eq!(c["status"], "AdjudicatedOverturned");
    e.transport
        .wait_after(c["challenge_deadline"].as_u64().unwrap())
        .unwrap();
    e.execute(Command::Finalize {
        run_id: "commands".into(),
        signer: REPORTER.into(),
        claim_id: 0,
    })
    .unwrap();
    e.execute(Command::Release {
        run_id: "commands".into(),
        signer: REPORTER.into(),
        assignment_seq: 0,
    })
    .unwrap();
    let c = e.balance(WORKER).unwrap();
    let amount = wbts_cli::number(&c["available"]).unwrap();
    assert!(amount >= NEAR);
    e.execute(Command::Withdraw {
        run_id: "commands".into(),
        signer: WORKER.into(),
        amount,
    })
    .unwrap();
    assert_eq!(e.balance(WORKER).unwrap()["available"], "1");
    let run = e
        .transport
        .view("get_run", json!({"run_id":"commands"}))
        .unwrap();
    assert!(
        wbts_cli::verify_journal(&e.journal, &run)
            .unwrap()
            .is_none()
    );
    e.journal.head = "00".repeat(32);
    let count = e.journal.transactions.len();
    assert!(e.anchor(REPORTER, "review", b"{}", Value::Null).is_err());
    assert_eq!(e.journal.transactions.len(), count);
}
#[test]
fn seeded_scenarios_and_both_s6_defenders_reach_wallet_recovery() {
    for (scenario, defender) in [
        ("s1", CHALLENGER),
        ("s2", CHALLENGER),
        ("s3", CHALLENGER),
        ("s5", CHALLENGER),
        ("s6", CHALLENGER),
        ("s6", WORKER),
    ] {
        let (mut e, _) = setup();
        e.execute(Command::RunDemo {
            scenario: scenario.into(),
            run_id: Some(format!(
                "{scenario}-{}",
                if defender == WORKER {
                    "self"
                } else {
                    "watcher"
                }
            )),
            reporter: REPORTER.into(),
            worker: WORKER.into(),
            challenger: defender.into(),
            registrar: Some(CHALLENGER.into()),
        })
        .unwrap();
        let p = e.journal.proof.as_ref().unwrap();
        assert_eq!(p["edge"]["released"], true);
        if scenario == "s6" {
            assert_eq!(p["corrected_claim"]["economic_outcome"], "Upheld");
            assert_eq!(p["after"]["reputations"][REPORTER], 75);
        }
        for id in [REPORTER, WORKER, CHALLENGER] {
            let c = e.balance(id).unwrap();
            assert_eq!(c["available"], "1");
            assert_eq!(c["in_flight"], "0");
        }
    }
}
#[test]
fn failed_command_saves_failure_without_submitting_later_steps() {
    let (mut e, _) = setup();
    let result = e.execute(Command::Register {
        run_id: Some("bad".into()),
        signer: REPORTER.into(),
        agents: vec![REPORTER.into(), WORKER.into(), WORKER.into()],
    });
    assert!(result.is_err());
    assert!(e.journal.transactions.is_empty());
}
