//! Seeded T2 fixtures and state/balance assertions; no contract modifications.
use super::*;
use borsh::BorshSerialize;
use std::path::Path;

pub const MALFORMED: &[u8] = br#"{"entries":["10000","25000","15000"],"declared_total":"49000"}"#;
pub const WRONG: &[u8] = br#"{"fee":"1225","net":"47775"}"#;

// Exact LogEvent Borsh field order from the pinned contract. AccountId is a string.
#[derive(Deserialize, BorshSerialize)]
struct Event {
    seq: u64,
    actor: String,
    kind: String,
    payload_hash: String,
    prev_hash: String,
    block_height: u64,
    task: Option<Link>,
}
#[derive(Deserialize, BorshSerialize)]
struct Link {
    assignment_seq: u64,
    worker: String,
    parent_assignment_seq: Option<u64>,
}
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct Mismatch {
    pub seq: u64,
    pub signer: String,
}

/// Recompute stored-event links/head and compare each local payload commitment.
/// The signer is the on-chain actor; a mismatch does not prove who edited a copy.
pub fn verify_journal(journal: &Journal, run: &Value) -> Result<Option<Mismatch>> {
    let events = run["events"].as_array().context("missing chain events")?;
    let mut head = String::new();
    for (i, value) in events.iter().enumerate() {
        let event: Event = serde_json::from_value(value.clone())?;
        let mismatch = || {
            Some(Mismatch {
                seq: i as u64,
                signer: event.actor.clone(),
            })
        };
        if event.seq != i as u64 || event.prev_hash != head {
            return Ok(mismatch());
        }
        let Some(local) = journal.events.get(i) else {
            return Ok(mismatch());
        };
        let bytes = local["payload_base64"]
            .as_str()
            .and_then(|s| STANDARD.decode(s).ok());
        if local["seq"] != event.seq
            || bytes.as_ref().is_none_or(|b| hash(b) != event.payload_hash)
            || local["payload_hash"] != event.payload_hash
        {
            return Ok(mismatch());
        }
        head = hash(&borsh::to_vec(&event)?);
    }
    if journal.events.len() != events.len() || run["head_hash"] != head || journal.head != head {
        return Ok(Some(Mismatch {
            seq: events.len() as u64,
            signer: "chain-head".into(),
        }));
    }
    Ok(None)
}

impl<T: Transport> Engine<T> {
    pub(super) fn funding_gate(
        &mut self,
        scenario: &str,
        registrar: &str,
        roles: [&str; 3],
        challenger: &str,
    ) -> Result<()> {
        let [coord, worker, watcher] = roles;
        let mut ids = vec![
            registrar.to_string(),
            coord.to_string(),
            worker.to_string(),
            watcher.to_string(),
        ];
        ids.sort();
        ids.dedup();
        for id in &ids {
            account(id)?;
        }
        let state = self.transport.snapshot(&ids)?;
        for id in &ids {
            let mut principal = 0;
            if id == coord {
                principal += NEAR;
            }
            if id == worker {
                principal += NEAR;
            }
            if scenario != "s3" {
                if id == if scenario == "s5" { worker } else { coord } {
                    principal += NEAR / 4;
                }
                if scenario != "s1" && id == if scenario == "s5" { coord } else { challenger } {
                    principal += NEAR / 4;
                }
                if scenario == "s6" && (id == worker || id == coord) {
                    principal += NEAR / 4;
                }
            }
            ensure!(
                number(&state["accounts"][id]["amount"])? >= principal + NEAR / 2,
                "funding gate: {id} requires independently funded collateral/bonds plus storage/gas headroom; no take started"
            );
        }
        Ok(())
    }

    pub(super) fn verify_run(&mut self, run_id: &str, copy: Option<&Path>) -> Result<()> {
        let path = copy.map(Path::to_path_buf).unwrap_or_else(|| {
            self.dir.join(format!(
                "{}.json",
                hash(format!("{}:{run_id}", self.contract).as_bytes())
            ))
        });
        let journal: Journal = serde_json::from_slice(&fs::read(path)?)?;
        ensure!(
            journal.contract == self.contract && journal.run_id == run_id,
            "journal identity mismatch"
        );
        let run = self.transport.view("get_run", json!({"run_id":run_id}))?;
        if let Some(m) = verify_journal(&journal, &run)? {
            println!("MISMATCH at seq {} signer={}", m.seq, m.signer);
            bail!("MISMATCH at seq {} signer={}", m.seq, m.signer);
        }
        println!("OK run={run_id} head={}", journal.head);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn seeded_demo(
        &mut self,
        scenario: &str,
        run: &str,
        coord: &str,
        worker: &str,
        watcher: &str,
        challenger: &str,
        registrar: &str,
    ) -> Result<()> {
        let start = Instant::now();
        ensure!(
            matches!(scenario, "s1" | "s2" | "s3" | "s5" | "s6"),
            "unsupported scenario"
        );
        let agents = vec![coord.to_string(), worker.to_string(), watcher.to_string()];
        self.transport.check_signers(&agents)?;
        self.transport.check_signers(&[registrar.into()])?;
        let params = self.parameters()?;
        ensure!(
            number(&params["stake"])? == NEAR
                && number(&params["blame_bond"])? == NEAR / 4
                && number(&params["challenge_bond"])? == NEAR / 4,
            "demo requires default economic terms"
        );
        let mut ids = agents.clone();
        ids.push(registrar.into());
        ids.sort();
        ids.dedup();
        let economic_start = self.transport.snapshot(&ids)?;
        let mut initial_credits = serde_json::Map::new();
        for id in &ids {
            initial_credits.insert(id.clone(), self.balance(id)?);
        }
        self.register(run, registrar, &agents)?;
        let snap = self.transport.snapshot(&agents)?;
        let height = snap["block_height"]
            .as_u64()
            .context("missing snapshot block height")?;
        let input = if matches!(scenario, "s5" | "s6") {
            MALFORMED
        } else {
            INPUT
        };
        let seq = self.offer(coord, worker, input, height + 100, height + 1000)?;
        self.accept(worker, seq)?;
        let task = json!({"assignment_seq":seq,"worker":worker,"parent_assignment_seq":null});
        self.anchor(
            worker,
            "settlement",
            if matches!(scenario, "s2" | "s3") {
                OUTPUT
            } else {
                WRONG
            },
            task,
        )?;
        let before = self.transport.snapshot(&agents)?;
        let mut claim = Value::Null;
        if scenario == "s3" {
            let chain = self.transport.view("get_run", json!({"run_id":run}))?;
            ensure!(
                verify_journal(&self.journal, &chain)?.is_none(),
                "intact journal failed"
            );
            self.verify_run(run, None)?;
            let mut copy: Journal = serde_json::from_value(serde_json::to_value(&self.journal)?)?;
            copy.events[1]["payload_base64"] = json!(STANDARD.encode(WRONG));
            let path = self.path.as_ref().unwrap().with_extension("tampered.json");
            fs::write(&path, serde_json::to_vec_pretty(&copy)?)?;
            ensure!(
                verify_journal(&copy, &chain)?
                    .context("tamper not detected")?
                    .seq
                    == 1,
                "wrong mismatch location"
            );
            ensure!(
                self.verify_run(run, Some(&path)).is_err(),
                "edited copy accepted"
            );
        } else {
            let (reporter, blamed, class, defender) = if scenario == "s5" {
                (worker, coord, "F3", coord)
            } else {
                (coord, worker, "F1", challenger)
            };
            let id = self.file_claim(reporter, blamed, class, &[0, 1])?;
            if scenario != "s1" {
                self.challenge(defender, id)?;
                let preview = self
                    .transport
                    .view("preview_adjudication", json!({"claim_id":id}))?;
                ensure!(
                    preview["verdict"]
                        == if scenario == "s5" {
                            "Upheld"
                        } else {
                            "Overturned"
                        },
                    "wrong verdict"
                );
                if matches!(scenario, "s5" | "s6") {
                    ensure!(
                        preview["finding"]["agent"] == coord
                            && preview["finding"]["failure_class"] == "F3",
                        "wrong cause"
                    );
                } else {
                    ensure!(preview["finding"].is_null(), "unexpected fault");
                }
                self.journal.preview = Some(preview);
                self.save()?;
            }
            claim = self.transport.view("get_claim", json!({"claim_id":id}))?;
            self.transport.wait_after(
                claim["challenge_deadline"]
                    .as_u64()
                    .context("missing deadline")?,
            )?;
            self.submit(reporter, "finalize", json!({"claim_id":id}), 0, false)?;
            claim = self.transport.view("get_claim", json!({"claim_id":id}))?;
            ensure!(
                claim["status"] == "Finalized"
                    && number(&claim["bond"])? == 0
                    && number(&claim["challenge_bond"])? == 0,
                "settlement incomplete"
            );
            if scenario == "s6" {
                let corrected = self.file_claim(worker, coord, "F3", &[0, 1])?;
                self.challenge(coord, corrected)?;
                let c = self
                    .transport
                    .view("get_claim", json!({"claim_id":corrected}))?;
                ensure!(
                    c["adjudication_result"]["verdict"] == "Upheld",
                    "corrected claim failed"
                );
                self.transport.wait_after(
                    c["challenge_deadline"]
                        .as_u64()
                        .context("missing deadline")?,
                )?;
                self.submit(worker, "finalize", json!({"claim_id":corrected}), 0, false)?;
                self.journal.proof = Some(
                    json!({"corrected_claim":self.transport.view("get_claim",json!({"claim_id":corrected}))?}),
                );
            }
        }
        let edge = self
            .transport
            .view("get_edge", json!({"edge_id":self.edge_id(seq)}))?;
        if edge["accepted_claim"].is_null() {
            let window = edge["window"].as_u64().context("missing window")?;
            let cutoff = edge["completion_height"]
                .as_u64()
                .context("missing height")?
                + window;
            self.transport
                .wait_after(cutoff.max(edge["corrected_deadline"].as_u64().unwrap_or(0)))?;
        }
        self.release(coord, seq)?;
        let released = self
            .transport
            .view("get_edge", json!({"edge_id":self.edge_id(seq)}))?;
        ensure!(
            released["released"] == true
                && number(&released["delegator_reserved"])? == 0
                && number(&released["worker_reserved"])? == 0,
            "reservations not released"
        );
        let mut credits = serde_json::Map::new();
        let mut ids = agents.clone();
        ids.push(registrar.into());
        ids.sort();
        ids.dedup();
        for id in &ids {
            let c = self.balance(id)?;
            if !c.is_null() {
                credits.insert(id.clone(), c.clone());
                let amount = number(&c["available"])?;
                if amount > 0 {
                    self.withdraw(id, amount)?;
                }
            }
        }
        let after = self.transport.snapshot(&ids)?;
        let mut economic_rows = Vec::new();
        for id in &ids {
            let start = &initial_credits[id];
            let end = self.balance(id)?;
            let initial_credit = if start.is_null() {
                0
            } else {
                number(&start["available"])?
            };
            let initial_storage = if start.is_null() {
                0
            } else {
                number(&start["retained_storage_paid"])?
            };
            let gas = self
                .journal
                .transactions
                .iter()
                .filter(|t| &t.signer == id)
                .try_fold(0u128, |sum, t| -> Result<u128> {
                    sum.checked_add(t.gas_yocto.parse()?)
                        .context("gas overflow")
                })?;
            let wallet = i128::try_from(number(&after["accounts"][id]["amount"])?)?
                - i128::try_from(number(&economic_start["accounts"][id]["amount"])?)?;
            let end_storage = if end.is_null() {
                0
            } else {
                number(&end["retained_storage_paid"])?
            };
            let end_credit = if end.is_null() {
                0
            } else {
                number(&end["available"])?
            };
            let storage = end_storage
                .checked_sub(initial_storage)
                .context("storage charges decreased")?;
            let actual = wallet
                + i128::try_from(gas)?
                + i128::try_from(storage)?
                + i128::try_from(end_credit)?
                - i128::try_from(initial_credit)?;
            let n = NEAR as i128;
            let expected = match scenario {
                "s1" => {
                    if id == coord {
                        n / 5
                    } else if id == worker {
                        -n / 4
                    } else {
                        0
                    }
                }
                "s2" => {
                    if id == coord {
                        -n / 4
                    } else if id == challenger {
                        n / 5
                    } else {
                        0
                    }
                }
                "s3" => 0,
                "s5" => {
                    if id == coord {
                        -n / 2
                    } else if id == worker {
                        n * 2 / 5
                    } else {
                        0
                    }
                }
                "s6" => {
                    if id == coord {
                        -n * 3 / 4
                    } else {
                        (if id == worker { n * 2 / 5 } else { 0 })
                            + (if id == challenger { n / 5 } else { 0 })
                    }
                }
                _ => unreachable!(),
            };
            ensure!(
                actual == expected,
                "{id} lifecycle economic mismatch: {actual}, expected {expected}; concurrent activity invalidates proof"
            );
            economic_rows.push(json!({"account":id,"economic_delta_yocto":actual.to_string(),"gas_yocto":gas.to_string(),"retained_storage_delta_yocto":storage.to_string(),"wallet_delta_yocto":wallet.to_string()}));
        }
        let treasury_delta = number(&after["treasury"])?
            .checked_sub(number(&economic_start["treasury"])?)
            .context("treasury decreased")?;
        ensure!(
            treasury_delta
                == match scenario {
                    "s3" => 0,
                    "s5" => NEAR / 10,
                    "s6" => NEAR * 15 / 100,
                    _ => NEAR / 20,
                },
            "treasury allocation mismatch"
        );
        let corrected = self
            .journal
            .proof
            .as_ref()
            .map(|v| v["corrected_claim"].clone())
            .unwrap_or(Value::Null);
        self.journal.proof = Some(
            json!({"before":before,"economic_start":economic_start,"economic_rows":economic_rows,"treasury_delta_yocto":treasury_delta.to_string(),"after":after,"claim":claim,"corrected_claim":corrected,"edge":released,"credits_before_withdraw":credits,"run":self.transport.view("get_run",json!({"run_id":run}))?,"elapsed_ms":start.elapsed().as_millis(),"storage_policy":"paid records retained; storage excluded from recovery","truth_policy":"unchallenged acceptance is optimistic"}),
        );
        self.save()?;
        println!(
            "{} PASS lifecycle: independent funding, settlement, release and confirmed withdrawals; evidence={}",
            scenario.to_uppercase(),
            self.path.as_ref().unwrap().display()
        );
        Ok(())
    }
}
