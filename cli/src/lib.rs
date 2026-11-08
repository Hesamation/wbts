mod harness;
pub mod live;
pub use harness::{Mismatch, verify_journal};

use anyhow::{Context, Result, bail, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::PathBuf,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

pub const NEAR: u128 = 1_000_000_000_000_000_000_000_000;
pub const PIN: &str = "6c8f4458183fc6f6e5c3d889d7f9ad07ef26ffbaafaa59d67406e15980b90444";
pub const SPEC: &[u8] = br#"{"task_type":"t2_fee_split_v1","fee_basis_points":250}"#;
pub const INPUT: &[u8] = br#"{"entries":["10000","25000","15000"],"declared_total":"50000"}"#;
pub const OUTPUT: &[u8] = br#"{"fee":"1250","net":"48750"}"#;

#[derive(Parser, Debug)]
#[command(about = "Testnet staked-attribution CLI; reads existing NEAR keystore keys only")]
pub struct App {
    #[arg(
        long,
        global = true,
        env = "WBTS_CONTRACT",
        default_value = "s2.coord-wbts.testnet"
    )]
    pub contract: String,
    #[arg(
        long,
        global = true,
        env = "WBTS_RPC_URL",
        default_value = "https://rpc.testnet.near.org"
    )]
    pub rpc_url: String,
    /// Public run artifacts only; no keys. Each run gets its own file.
    #[arg(long, global = true, default_value = ".wbts")]
    pub journal_dir: PathBuf,
    /// Upper bound for extra storage funding per payable call, in yoctoNEAR.
    #[arg(long, global = true, default_value = "50000000000000000000000")]
    pub storage_allowance: u128,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    Register {
        #[arg(long)]
        run_id: Option<String>,
        #[arg(long, default_value = "coord-wbts.testnet")]
        signer: String,
        #[arg(long, num_args = 3)]
        agents: Vec<String>,
    },
    /// Hash compact, recursively key-sorted JSON and append one event.
    Anchor {
        #[arg(long)]
        run_id: String,
        #[arg(long)]
        signer: String,
        #[arg(long)]
        kind: String,
        #[arg(long)]
        payload: PathBuf,
        #[arg(long)]
        worker: Option<String>,
        #[arg(long)]
        assignment_seq: Option<u64>,
    },
    FileClaim {
        #[arg(long)]
        run_id: String,
        #[arg(long)]
        signer: String,
        #[arg(long)]
        blamed: String,
        #[arg(long, default_value = "F1", value_parser = ["F1", "F3"])]
        failure_class: String,
        #[arg(long, value_delimiter = ',')]
        evidence_seqs: Vec<u64>,
    },
    Challenge {
        #[arg(long)]
        run_id: String,
        #[arg(long)]
        signer: String,
        #[arg(long)]
        claim_id: u64,
    },
    /// Recompute a local journal against immutable chain events and head.
    VerifyRun {
        #[arg(long)]
        run_id: String,
        /// Optional local copy, e.g. an intentionally edited S3 journal.
        #[arg(long)]
        journal: Option<PathBuf>,
    },
    RunDemo {
        #[arg(value_parser = ["s1", "s2", "s3", "s5", "s6"])]
        scenario: String,
        #[arg(long)]
        run_id: Option<String>,
        #[arg(long, default_value = "coord-wbts.testnet")]
        reporter: String,
        #[arg(long, default_value = "worker-wbts.testnet")]
        worker: String,
        #[arg(long, default_value = "watcher")]
        challenger: String,
        /// Stake sponsor; defaults to reporter. Does not change settlement roles.
        #[arg(long)]
        registrar: Option<String>,
    },
}

pub fn account(id: &str) -> Result<()> {
    id.parse::<near_primitives::types::AccountId>()
        .context("invalid account ID")?;
    ensure!(
        id.ends_with(".testnet"),
        "only named .testnet accounts are supported"
    );
    Ok(())
}

impl App {
    pub fn validate(&self) -> Result<()> {
        account(&self.contract)?;
        ensure!(
            self.storage_allowance <= NEAR / 10 && self.storage_allowance > 0,
            "storage allowance must be >0 and <=0.1 NEAR"
        );
        Ok(())
    }
}

pub fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
pub fn canonical(bytes: &[u8]) -> Result<Vec<u8>> {
    // Sort explicitly, including when another crate enables preserve_order.
    fn sort(value: &mut Value) {
        match value {
            Value::Object(map) => {
                let sorted: std::collections::BTreeMap<_, _> =
                    std::mem::take(map).into_iter().collect();
                for (key, mut value) in sorted {
                    sort(&mut value);
                    map.insert(key, value);
                }
            }
            Value::Array(values) => {
                for value in values {
                    sort(value);
                }
            }
            _ => {}
        }
    }
    let mut value: Value = serde_json::from_slice(bytes).context("payload must be JSON")?;
    sort(&mut value);
    Ok(serde_json::to_vec(&value)?)
}
pub fn number(value: &Value) -> Result<u128> {
    value
        .as_str()
        .context("expected decimal-string amount")?
        .parse()
        .context("invalid amount")
}
fn fresh() -> String {
    format!(
        "s2-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Tx {
    pub hash: String,
    pub signer: String,
    pub method: String,
    pub block_height: u64,
    pub elapsed_ms: u128,
    pub started_unix_ms: u128,
    pub ended_unix_ms: u128,
    pub gas_burnt: u64,
    pub gas_yocto: String,
    pub storage_bytes: u64,
    pub storage_yocto: String,
    pub attached_yocto: String,
    pub excess_refunded_yocto: String,
    pub result: Value,
    pub outcome: Value,
}

pub trait Transport {
    fn view(&mut self, method: &str, args: Value) -> Result<Value>;
    fn call(
        &mut self,
        signer: &str,
        method: &str,
        args: Value,
        principal: u128,
        allowance: u128,
    ) -> Result<Tx>;
    fn snapshot(&mut self, accounts: &[String]) -> Result<Value>;
    fn wait_after(&mut self, deadline: u64) -> Result<()>;
    fn check_signers(&mut self, accounts: &[String]) -> Result<()>;
}

#[derive(Default, Serialize, Deserialize)]
pub struct Journal {
    pub contract: String,
    pub run_id: String,
    pub head: String,
    pub events: Vec<Value>,
    pub transactions: Vec<Tx>,
    pub preview: Option<Value>,
    pub proof: Option<Value>,
    pub failure: Option<String>,
}

pub struct Engine<T: Transport> {
    pub transport: T,
    contract: String,
    dir: PathBuf,
    allowance: u128,
    pub journal: Journal,
    path: Option<PathBuf>,
}

impl<T: Transport> Engine<T> {
    pub fn new(transport: T, app: &App) -> Result<Self> {
        app.validate()?;
        Ok(Self {
            transport,
            contract: app.contract.clone(),
            dir: app.journal_dir.clone(),
            allowance: app.storage_allowance,
            journal: Journal::default(),
            path: None,
        })
    }
    fn open(&mut self, run: &str, create: bool) -> Result<()> {
        ensure!(
            !run.is_empty() && run.len() <= 128,
            "run ID must have 1..128 bytes"
        );
        fs::create_dir_all(&self.dir)?;
        let path = self.dir.join(format!(
            "{}.json",
            hash(format!("{}:{run}", self.contract).as_bytes())
        ));
        if create {
            ensure!(
                !path.exists(),
                "journal already exists; inspect it before retrying"
            );
            ensure!(
                self.transport
                    .view("get_run", json!({"run_id": run}))?
                    .is_null(),
                "run already exists on chain"
            );
            // create_new also prevents two processes from registering the same local run.
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?;
            self.journal = Journal {
                contract: self.contract.clone(),
                run_id: run.into(),
                ..Default::default()
            };
        } else {
            self.journal = serde_json::from_slice(
                &fs::read(&path).context("local journal missing; do not invent an anchor head")?,
            )?;
            ensure!(
                self.journal.contract == self.contract && self.journal.run_id == run,
                "journal identity mismatch"
            );
            ensure!(
                self.journal.failure.is_none(),
                "previous command failed; inspect journal and chain before continuing"
            );
        }
        self.path = Some(path);
        self.save()
    }
    fn save(&self) -> Result<()> {
        if let Some(path) = &self.path {
            let temp = path.with_extension(format!("{}.tmp", std::process::id()));
            fs::write(&temp, serde_json::to_vec_pretty(&self.journal)?)?;
            fs::rename(temp, path)?;
        }
        Ok(())
    }
    fn submit(
        &mut self,
        signer: &str,
        method: &str,
        args: Value,
        principal: u128,
        payable: bool,
    ) -> Result<Tx> {
        account(signer)?;
        println!("Starting {method} signer={signer}");
        let tx = self
            .transport
            .call(
                signer,
                method,
                args,
                principal,
                if payable { self.allowance } else { 0 },
            )
            .with_context(|| format!("{method} failed; no later step submitted"))?;
        println!(
            "{method} Success tx={} block={} started_unix_ms={} elapsed_ms={} gas={} gas_yocto={} storage_bytes={} storage_yocto={} excess_refunded_yocto={}",
            tx.hash,
            tx.block_height,
            tx.started_unix_ms,
            tx.elapsed_ms,
            tx.gas_burnt,
            tx.gas_yocto,
            tx.storage_bytes,
            tx.storage_yocto,
            tx.excess_refunded_yocto
        );
        self.journal.transactions.push(tx.clone());
        self.save()?;
        Ok(tx)
    }
    fn parameters(&mut self) -> Result<Value> {
        self.transport.view("get_parameters", json!({}))
    }
    fn pin(&mut self) -> Result<()> {
        let info = self.transport.view("get_verifier_info", json!({}))?;
        ensure!(
            info["verifier_hash"] == PIN
                && info["identity_kind"] == "source-and-dependency-manifest-sha256-v1",
            "verifier pin mismatch"
        );
        Ok(())
    }
    pub fn register(&mut self, run: &str, signer: &str, agents: &[String]) -> Result<()> {
        ensure!(agents.len() == 3, "register requires three agents");
        for agent in agents {
            account(agent)?;
        }
        ensure!(
            agents[0] != agents[1] && agents[0] != agents[2] && agents[1] != agents[2],
            "agents must be distinct"
        );
        account(signer)?;
        self.pin()?;
        let params = self.parameters()?;
        let stake = number(&params["stake"])?;
        ensure!(stake == NEAR, "expected 1 NEAR participation stake");
        self.transport.check_signers(&[signer.into()])?;
        self.open(run, true)?;
        self.submit(
            signer,
            "register",
            json!({"run_id":run,"agents":agents,"task_spec":STANDARD.encode(SPEC),
            "verifier":{"task_spec_hash":hash(SPEC),"verifier_hash":PIN,"verifier_key":null}}),
            stake.checked_mul(3).context("stake overflow")?,
            true,
        )?;
        let view = self.transport.view("get_run", json!({"run_id":run}))?;
        ensure!(
            view["agents"].as_array().context("missing agents")?.len() == 3,
            "registration view mismatch"
        );
        for agent in view["agents"].as_array().unwrap() {
            ensure!(number(&agent["locked_stake"])? == stake, "incorrect stake");
        }
        Ok(())
    }
    pub fn anchor(&mut self, signer: &str, kind: &str, bytes: &[u8], task: Value) -> Result<()> {
        let run = self.journal.run_id.clone();
        let view = self.transport.view("get_run", json!({"run_id":run}))?;
        let seq = self.journal.events.len();
        ensure!(
            view["head_hash"] == self.journal.head
                && view["events"].as_array().context("missing events")?.len() == seq,
            "local head differs from chain; stopped before submission"
        );
        let bytes = canonical(bytes)?;
        let payload_hash = hash(&bytes);
        let tx = self.submit(
            signer,
            "anchor",
            json!({"run_id":run,"seq":seq,"actor":signer,"kind":kind,
            "payload_hash":payload_hash,"prev_hash":self.journal.head,"task":task}),
            0,
            true,
        )?;
        self.journal.head = tx
            .result
            .as_str()
            .context("anchor did not return head")?
            .into();
        self.journal.events.push(json!({"seq":seq,"payload_base64":STANDARD.encode(&bytes),"payload_hash":payload_hash,"tx_hash":tx.hash}));
        self.save()?;
        let view = self.transport.view("get_run", json!({"run_id":run}))?;
        ensure!(
            view["head_hash"] == self.journal.head,
            "confirmed anchor head mismatch"
        );
        Ok(())
    }
    pub fn file_claim(
        &mut self,
        signer: &str,
        blamed: &str,
        class: &str,
        seqs: &[u64],
    ) -> Result<u64> {
        account(blamed)?;
        ensure!(
            matches!(class, "F1" | "F3"),
            "only direct T2 F1/F3 supported"
        );
        ensure!(
            !seqs.is_empty() && seqs.windows(2).all(|w| w[0] < w[1]),
            "evidence sequences must be increasing and nonempty"
        );
        let mut hashes = Vec::new();
        let mut txs = Vec::new();
        for seq in seqs {
            let e = self
                .journal
                .events
                .get(usize::try_from(*seq)?)
                .context("evidence sequence missing locally")?;
            hashes.push(e["payload_hash"].clone());
            txs.push(e["tx_hash"].clone());
        }
        let bond = number(&self.parameters()?["blame_bond"])?;
        let tx = self.submit(signer, "file_claim", json!({"run_id":self.journal.run_id,"blamed":blamed,"failure_class":class,
            "verifier_hash":PIN,"evidence_seqs":seqs,"evidence_payload_hashes":hashes,"evidence_tx_hashes":txs}), bond, true)?;
        let id = tx.result.as_u64().context("file_claim did not return ID")?;
        let claim = self.transport.view("get_claim", json!({"claim_id":id}))?;
        ensure!(
            claim["status"] == "Pending" && number(&claim["bond"])? == bond,
            "claim view mismatch"
        );
        Ok(id)
    }
    pub fn challenge(&mut self, signer: &str, id: u64) -> Result<()> {
        let claim = self.transport.view("get_claim", json!({"claim_id":id}))?;
        ensure!(
            claim["run_id"] == self.journal.run_id && claim["status"] == "Pending",
            "claim not pending in this run"
        );
        let bond = number(&self.parameters()?["challenge_bond"])?;
        self.submit(signer, "challenge", json!({"claim_id":id}), bond, true)?;
        let claim = self.transport.view("get_claim", json!({"claim_id":id}))?;
        ensure!(
            claim["status"] == "Challenged" && number(&claim["challenge_bond"])? == bond,
            "challenge view mismatch"
        );
        Ok(())
    }
    pub fn s2(
        &mut self,
        run: &str,
        reporter: &str,
        worker: &str,
        challenger: &str,
        registrar: &str,
    ) -> Result<()> {
        let start = Instant::now();
        let agents: Vec<String> = vec![reporter.into(), worker.into(), challenger.into()];
        for a in &agents {
            account(a)?;
        }
        account(registrar)?;
        self.transport.check_signers(&agents)?;
        self.transport.check_signers(&[registrar.into()])?;
        let params = self.parameters()?;
        ensure!(
            number(&params["blame_bond"])? == NEAR / 4
                && number(&params["challenge_bond"])? == NEAR / 4,
            "S2 requires 0.25 NEAR bonds"
        );
        println!(
            "S2 run={run} contract={} (creator funds 3 NEAR locked participation stakes)",
            self.contract
        );
        self.register(run, registrar, &agents)?;
        let task = json!({"assignment_seq":0,"worker":worker,"parent_assignment_seq":null});
        self.anchor(reporter, "task_assigned", INPUT, task.clone())?;
        self.anchor(worker, "settlement", OUTPUT, task)?;
        let before = self.transport.snapshot(&agents)?;
        let begin = self.journal.transactions.len();
        // Persist the accounting baseline before risking the blame bond.
        self.journal.proof = Some(json!({"before":before}));
        self.save()?;
        let id = self.file_claim(reporter, worker, "F1", &[0, 1])?;
        let filed = self.transport.view("get_claim", json!({"claim_id":id}))?;
        ensure!(
            filed["challenge_deadline"]
                .as_u64()
                .context("missing deadline")?
                .checked_sub(
                    filed["claim_height"]
                        .as_u64()
                        .context("missing claim height")?
                )
                == Some(600),
            "S2 requires the 600-block challenge window"
        );
        self.challenge(challenger, id)?;
        let args = json!({"claim_id":id,"task_spec":STANDARD.encode(SPEC),"payloads":self.journal.events.iter().map(|e| json!({"seq":e["seq"],"bytes":e["payload_base64"]})).collect::<Vec<_>>()});
        let preview = self.transport.view("preview_adjudication", args.clone())?;
        self.journal.preview = Some(preview.clone());
        self.save()?;
        ensure!(
            preview["verdict"] == "Overturned" && preview["finding"].is_null(),
            "S2 preview must overturn with no finding"
        );
        println!("preview_adjudication Overturned finding=null");
        let persisted = self.submit(challenger, "adjudicate", args, 0, false)?;
        ensure!(
            persisted.result == preview,
            "persisted result differs from preview"
        );
        let claim = self.transport.view("get_claim", json!({"claim_id":id}))?;
        ensure!(
            claim["status"] == "AdjudicatedOverturned" && claim["corrected_attribution"].is_null(),
            "unexpected adjudication state"
        );
        self.transport.wait_after(
            claim["challenge_deadline"]
                .as_u64()
                .context("missing deadline")?,
        )?;
        self.submit(reporter, "finalize", json!({"claim_id":id}), 0, false)?;
        let after = self.transport.snapshot(&agents)?;
        let final_claim = self.transport.view("get_claim", json!({"claim_id":id}))?;
        let final_run = self.transport.view("get_run", json!({"run_id":run}))?;
        self.journal.proof = Some(
            json!({"before":before,"after":after,"claim":final_claim,"run":final_run,"elapsed_ms":start.elapsed().as_millis()}),
        );
        self.save()?;
        ensure!(
            final_claim["status"] == "Finalized"
                && number(&final_claim["bond"])? == 0
                && number(&final_claim["challenge_bond"])? == 0,
            "finalized bonds not cleared"
        );
        ensure!(
            final_claim["corrected_attribution"].is_null(),
            "unexpected corrected attribution"
        );
        for stake in final_run["agents"].as_array().context("missing stakes")? {
            ensure!(number(&stake["locked_stake"])? == NEAR, "S2 stake changed");
        }
        let proof = verify_money(
            &before,
            &after,
            &agents,
            &self.journal.transactions[begin..],
        )?;
        self.journal.proof = Some(
            json!({"before":before,"after":after,"verification":proof,"claim":final_claim,"run":final_run,"elapsed_ms":start.elapsed().as_millis()}),
        );
        self.save()?;
        println!(
            "S2 PASS case=3 reporter=-0.25 challenger=+0.20 treasury=+0.05 NEAR; reputation verified from starting scores; elapsed_ms={}",
            start.elapsed().as_millis()
        );
        println!("Evidence: {}", self.path.as_ref().unwrap().display());
        Ok(())
    }
    pub fn execute(&mut self, command: Command) -> Result<()> {
        let result = (|| match command {
            Command::Register {
                run_id,
                signer,
                agents,
            } => {
                let agents = if agents.is_empty() {
                    vec![
                        signer.clone(),
                        "worker-wbts.testnet".into(),
                        "watcher-wbts.testnet".into(),
                    ]
                } else {
                    agents
                };
                self.register(&run_id.unwrap_or_else(fresh), &signer, &agents)
            }
            Command::Anchor {
                run_id,
                signer,
                kind,
                payload,
                worker,
                assignment_seq,
            } => {
                self.open(&run_id, false)?;
                let task = match (worker, assignment_seq) {
                    (Some(w), Some(s)) => {
                        account(&w)?;
                        json!({"assignment_seq":s,"worker":w,"parent_assignment_seq":null})
                    }
                    (None, None) => Value::Null,
                    _ => bail!("worker and assignment-seq must be supplied together"),
                };
                self.anchor(&signer, &kind, &fs::read(payload)?, task)
            }
            Command::FileClaim {
                run_id,
                signer,
                blamed,
                failure_class,
                evidence_seqs,
            } => {
                self.open(&run_id, false)?;
                let id = self.file_claim(&signer, &blamed, &failure_class, &evidence_seqs)?;
                println!("claim_id={id}");
                Ok(())
            }
            Command::Challenge {
                run_id,
                signer,
                claim_id,
            } => {
                self.open(&run_id, false)?;
                self.challenge(&signer, claim_id)
            }
            Command::VerifyRun { run_id, journal } => self.verify_run(&run_id, journal.as_deref()),
            Command::RunDemo {
                scenario,
                run_id,
                reporter,
                worker,
                challenger,
                registrar,
            } => {
                let watcher = "watcher-wbts.testnet";
                let selected = match challenger.as_str() {
                    "watcher" => watcher,
                    "worker" => &worker,
                    other => other,
                };
                let registrar = registrar.as_deref().unwrap_or(&reporter);
                let run = run_id.unwrap_or_else(|| fresh().replacen("s2", &scenario, 1));
                // Account-wide gate precedes the first transaction, including S2.
                let watcher = if selected == worker {
                    watcher
                } else {
                    selected
                };
                self.funding_gate(
                    &scenario,
                    registrar,
                    [&reporter, &worker, watcher],
                    selected,
                )?;
                if scenario == "s2" {
                    self.s2(&run, &reporter, &worker, selected, registrar)
                } else {
                    self.seeded_demo(
                        &scenario, &run, &reporter, &worker, watcher, selected, registrar,
                    )
                }
            }
        })();
        if let Err(error) = &result {
            self.journal.failure = Some(format!("{error:#}"));
            self.save()?;
        }
        result
    }
}

pub fn verify_money(before: &Value, after: &Value, agents: &[String], txs: &[Tx]) -> Result<Value> {
    ensure!(
        agents.len() == 3
            && agents[0] != agents[1]
            && agents[0] != agents[2]
            && agents[1] != agents[2],
        "money proof requires three distinct roles"
    );
    let expected = [-(NEAR as i128 / 4), 0, NEAR as i128 / 5];
    let mut rows = Vec::new();
    for (i, a) in agents.iter().enumerate() {
        let mut gas = 0u128;
        let mut storage = 0u128;
        for tx in txs.iter().filter(|tx| &tx.signer == a) {
            gas = gas
                .checked_add(tx.gas_yocto.parse()?)
                .context("gas overflow")?;
            storage = storage
                .checked_add(tx.storage_yocto.parse()?)
                .context("storage overflow")?;
        }
        let delta = i128::try_from(number(&after["accounts"][a]["amount"])?)?
            - i128::try_from(number(&before["accounts"][a]["amount"])?)?;
        let economic = delta + i128::try_from(gas)? + i128::try_from(storage)?;
        ensure!(
            economic == expected[i],
            "{a} economic balance mismatch: {economic}, expected {} (concurrent account activity invalidates proof)",
            expected[i]
        );
        let rep = before["reputations"][a]
            .as_u64()
            .context("missing initial reputation")?;
        let wanted = match i {
            0 => rep.saturating_sub(15),
            1 => (rep + 2).min(120),
            _ => (rep + 5).min(120),
        };
        ensure!(after["reputations"][a] == wanted, "{a} reputation mismatch");
        rows.push(json!({"account":a,"balance_delta_yocto":delta.to_string(),"gas_yocto":gas.to_string(),"storage_yocto":storage.to_string(),"economic_delta_yocto":economic.to_string(),"reputation_before":rep,"reputation_after":wanted}));
    }
    let treasury = number(&after["treasury"])?
        .checked_sub(number(&before["treasury"])?)
        .context("treasury decreased")?;
    ensure!(treasury == NEAR / 20, "treasury delta mismatch");
    Ok(
        json!({"roles":rows,"treasury_delta_yocto":treasury.to_string(),"payout_yocto":(NEAR*45/100).to_string(),"bond_plus_challenge_yocto":(NEAR/2).to_string(),"payout_plus_treasury_yocto":(NEAR/2).to_string()}),
    )
}
