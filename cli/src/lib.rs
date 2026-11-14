pub mod data;
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
pub const PIN: &str = "63b48b0688e1fac70dbb2c2ee7de40e952902296e07268d6a38fe3b771450bc9";
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
    /// Hash exact payload bytes and append an event (assignments use offer).
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
    /// Offer an edge with caller-owned collateral and explicit immutable deadlines.
    Offer {
        #[arg(long)]
        run_id: String,
        #[arg(long)]
        signer: String,
        #[arg(long)]
        worker: String,
        #[arg(long)]
        payload: PathBuf,
        #[arg(long)]
        accept_deadline: u64,
        #[arg(long)]
        work_deadline: u64,
    },
    Accept {
        #[arg(long)]
        run_id: String,
        #[arg(long)]
        signer: String,
        #[arg(long)]
        assignment_seq: u64,
    },
    Complete {
        #[arg(long)]
        run_id: String,
        #[arg(long)]
        signer: String,
        #[arg(long)]
        assignment_seq: u64,
        #[arg(long)]
        payload: PathBuf,
    },
    Release {
        #[arg(long)]
        run_id: String,
        #[arg(long)]
        signer: String,
        #[arg(long)]
        assignment_seq: u64,
    },
    Finalize {
        #[arg(long)]
        run_id: String,
        #[arg(long)]
        signer: String,
        #[arg(long)]
        claim_id: u64,
    },
    Edge {
        #[arg(long)]
        run_id: String,
        #[arg(long)]
        assignment_seq: u64,
    },
    Preview {
        #[arg(long)]
        claim_id: u64,
    },
    Balance {
        #[arg(long)]
        account_id: String,
    },
    Withdraw {
        #[arg(long)]
        run_id: String,
        #[arg(long)]
        signer: String,
        #[arg(long)]
        amount: u128,
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
        /// Run registrar only; every participant funds its own edge reservation.
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
            self.storage_allowance <= NEAR / 4 && self.storage_allowance > 0,
            "storage allowance must be >0 and <=0.25 NEAR"
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
    #[serde(default)]
    pub excess_refunded_yocto: String,
    #[serde(default)]
    pub excess_credited_yocto: String,
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
    /// True when historical payload bytes are unavailable in this participant journal.
    #[serde(default)]
    pub partial: bool,
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
    // Non-filing participants need neither the creator's files nor its keys.
    fn open_action(&mut self, run: &str) -> Result<()> {
        ensure!(
            !run.is_empty() && run.len() <= 128,
            "run ID must have 1..128 bytes"
        );
        let path = self.dir.join(format!(
            "{}.json",
            hash(format!("{}:{run}", self.contract).as_bytes())
        ));
        if path.exists() {
            return self.open(run, false);
        }
        let chain = self.transport.view("get_run", json!({"run_id":run}))?;
        ensure!(!chain.is_null(), "unknown run");
        let events=chain["events"].as_array().context("missing events")?.iter().map(|e|json!({"seq":e["seq"],"payload_hash":e["payload_hash"],"payload_base64":null,"tx_hash":null})).collect();
        fs::create_dir_all(&self.dir)?;
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        self.journal = Journal {
            contract: self.contract.clone(),
            run_id: run.into(),
            head: chain["head_hash"].as_str().context("missing head")?.into(),
            partial: true,
            events,
            ..Default::default()
        };
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
            "{method} Success tx={} block={} started_unix_ms={} elapsed_ms={} gas={} gas_yocto={} storage_bytes={} storage_yocto={} excess_credited_yocto={}",
            tx.hash,
            tx.block_height,
            tx.started_unix_ms,
            tx.elapsed_ms,
            tx.gas_burnt,
            tx.gas_yocto,
            tx.storage_bytes,
            tx.storage_yocto,
            tx.excess_credited_yocto
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
        self.transport.check_signers(&[signer.into()])?;
        self.open(run, true)?;
        self.submit(
            signer,
            "register",
            json!({"run_id":run,"agents":agents,"task_spec":STANDARD.encode(SPEC),
            "verifier":{"task_spec_hash":hash(SPEC),"verifier_hash":PIN,"verifier_key":null}}),
            0,
            true,
        )?;
        let view = self.transport.view("get_run", json!({"run_id":run}))?;
        ensure!(
            view["agents"].as_array().context("missing agents")?.len() == 3,
            "registration view mismatch"
        );
        Ok(())
    }
    pub fn offer(
        &mut self,
        signer: &str,
        worker: &str,
        bytes: &[u8],
        accept_deadline: u64,
        work_deadline: u64,
    ) -> Result<u64> {
        account(worker)?;
        let run = self.journal.run_id.clone();
        let view = self.transport.view("get_run", json!({"run_id":run}))?;
        let seq = self.journal.events.len() as u64;
        ensure!(
            view["head_hash"] == self.journal.head
                && view["events"].as_array().context("missing events")?.len() == seq as usize,
            "local head differs from chain"
        );
        let stake = number(&self.parameters()?["stake"])?;
        let tx=self.submit(signer,"offer_assignment",json!({"run_id":run,"worker":worker,"payload_hash":hash(bytes),"accept_deadline":accept_deadline,"work_deadline":work_deadline,"seq":seq,"prev_hash":self.journal.head}),stake,true)?;
        ensure!(
            tx.result == json!({"run_id":run,"assignment_seq":seq}),
            "offer returned wrong edge"
        );
        let view = self.transport.view("get_run", json!({"run_id":run}))?;
        self.journal.head = view["head_hash"].as_str().context("missing head")?.into();
        self.journal.events.push(json!({"seq":seq,"payload_base64":STANDARD.encode(bytes),"payload_hash":hash(bytes),"tx_hash":tx.hash}));
        self.save()?;
        Ok(seq)
    }
    fn edge_id(&self, seq: u64) -> Value {
        json!({"run_id":self.journal.run_id,"assignment_seq":seq})
    }
    pub fn accept(&mut self, signer: &str, seq: u64) -> Result<()> {
        let id = self.edge_id(seq);
        let edge = self.transport.view("get_edge", json!({"edge_id":id}))?;
        ensure!(edge["worker"] == signer, "only assigned worker may accept");
        self.submit(
            signer,
            "accept_assignment",
            json!({"edge_id":id}),
            number(&edge["parameters"]["stake"])?,
            true,
        )?;
        Ok(())
    }
    pub fn release(&mut self, signer: &str, seq: u64) -> Result<()> {
        self.submit(
            signer,
            "release_edge",
            json!({"edge_id":self.edge_id(seq)}),
            0,
            false,
        )?;
        Ok(())
    }
    pub fn balance(&mut self, signer: &str) -> Result<Value> {
        account(signer)?;
        self.transport
            .view("get_credit", json!({"account_id":signer}))
    }
    pub fn withdraw(&mut self, signer: &str, amount: u128) -> Result<()> {
        let before = self.balance(signer)?;
        ensure!(
            number(&before["in_flight"])? == 0
                && amount > 0
                && amount <= number(&before["available"])?,
            "invalid withdrawal or pending transfer"
        );
        let tx = self.submit(
            signer,
            "withdraw",
            json!({"amount":amount.to_string()}),
            1,
            false,
        )?;
        let after = self.balance(signer)?;
        ensure!(
            number(&after["in_flight"])? == 0,
            "withdrawal outcome pending; inspect chain before retrying"
        );
        ensure!(
            tx.result == true,
            "withdrawal failed; credit restored, inspect before retrying"
        );
        ensure!(
            number(&after["available"])? == number(&before["available"])? + 1 - amount,
            "withdrawal credit mismatch"
        );
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
        let bytes = bytes.to_vec();
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
        let seq = seqs[0];
        let edge = self
            .transport
            .view("get_edge", json!({"edge_id":self.edge_id(seq)}))?;
        let expected = if class == "F1" {
            &edge["worker"]
        } else {
            &edge["delegator"]
        };
        ensure!(
            expected == blamed,
            "blamed account does not match edge role"
        );
        let mut payloads = Vec::new();
        let mut txs = Vec::new();
        for seq in seqs {
            let e = self
                .journal
                .events
                .get(usize::try_from(*seq)?)
                .context("evidence missing locally")?;
            ensure!(
                e["payload_base64"].is_string() && e["tx_hash"].is_string(),
                "exact evidence bytes/references unavailable locally; obtain the producer journal before filing"
            );
            payloads.push(json!({"seq":seq,"bytes":e["payload_base64"]}));
            txs.push(e["tx_hash"].clone());
        }
        let bond = number(&edge["parameters"]["blame_bond"])?;
        let tx=self.submit(signer,"file_claim",json!({"edge_id":self.edge_id(seq),"failure_class":class,"payloads":payloads,"evidence_tx_hashes":txs}),bond,true)?;
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
            claim["edge_id"]["run_id"] == self.journal.run_id && claim["status"] == "Pending",
            "claim not pending in this run"
        );
        let edge = self
            .transport
            .view("get_edge", json!({"edge_id":claim["edge_id"]}))?;
        let bond = number(&edge["parameters"]["challenge_bond"])?;
        self.submit(signer, "challenge", json!({"claim_id":id}), bond, true)?;
        let claim = self.transport.view("get_claim", json!({"claim_id":id}))?;
        ensure!(
            matches!(
                claim["status"].as_str(),
                Some("AdjudicatedUpheld" | "AdjudicatedOverturned")
            ) && number(&claim["challenge_bond"])? == bond,
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
        self.seeded_demo(
            "s2", run, reporter, worker, challenger, challenger, registrar,
        )
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
            Command::Offer {
                run_id,
                signer,
                worker,
                payload,
                accept_deadline,
                work_deadline,
            } => {
                self.open(&run_id, false)?;
                let seq = self.offer(
                    &signer,
                    &worker,
                    &fs::read(payload)?,
                    accept_deadline,
                    work_deadline,
                )?;
                println!("assignment_seq={seq}");
                Ok(())
            }
            Command::Accept {
                run_id,
                signer,
                assignment_seq,
            } => {
                self.open_action(&run_id)?;
                self.accept(&signer, assignment_seq)
            }
            Command::Complete {
                run_id,
                signer,
                assignment_seq,
                payload,
            } => {
                self.open_action(&run_id)?;
                self.anchor(&signer,"settlement",&fs::read(payload)?,json!({"assignment_seq":assignment_seq,"worker":signer,"parent_assignment_seq":null}))
            }
            Command::Release {
                run_id,
                signer,
                assignment_seq,
            } => {
                self.open_action(&run_id)?;
                self.release(&signer, assignment_seq)
            }
            Command::Finalize {
                run_id,
                signer,
                claim_id,
            } => {
                self.open_action(&run_id)?;
                self.submit(&signer, "finalize", json!({"claim_id":claim_id}), 0, false)?;
                Ok(())
            }
            Command::Edge {
                run_id,
                assignment_seq,
            } => {
                let value = self.transport.view(
                    "get_edge",
                    json!({"edge_id":{"run_id":run_id,"assignment_seq":assignment_seq}}),
                )?;
                println!("{}", serde_json::to_string_pretty(&value)?);
                Ok(())
            }
            Command::Preview { claim_id } => {
                let value = self
                    .transport
                    .view("preview_adjudication", json!({"claim_id":claim_id}))?;
                println!("{}", serde_json::to_string_pretty(&value)?);
                Ok(())
            }
            Command::Balance { account_id } => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&self.balance(&account_id)?)?
                );
                Ok(())
            }
            Command::Withdraw {
                run_id,
                signer,
                amount,
            } => {
                self.open_action(&run_id)?;
                self.withdraw(&signer, amount)
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
                self.open_action(&run_id)?;
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
