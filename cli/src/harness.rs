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
            registrar.to_owned(),
            coord.to_owned(),
            worker.to_owned(),
            watcher.to_owned(),
        ];
        ids.sort();
        ids.dedup();
        for id in &ids {
            account(id)?;
        }
        let state = self.transport.snapshot(&ids)?;
        for id in &ids {
            let bonds = if scenario == "s3" {
                vec![]
            } else if scenario == "s5" {
                vec![worker, coord]
            } else if scenario == "s1" {
                vec![coord]
            } else {
                vec![coord, challenger]
            };
            let minimum = if id == registrar {
                NEAR * 31 / 10
                    + if bonds.contains(&id.as_str()) {
                        NEAR * 3 / 10
                    } else {
                        0
                    }
            } else {
                NEAR * 3 / 10
            };
            ensure!(
                number(&state["accounts"][id]["amount"])? >= minimum + NEAR / 100,
                "funding gate: {id} needs {} yoctoNEAR plus 0.01 NEAR gas headroom; no take started",
                minimum
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
        ensure!(
            matches!(scenario, "s1" | "s3" | "s5" | "s6"),
            "unsupported scenario"
        );
        let start = Instant::now();
        let agents = vec![coord.to_owned(), worker.to_owned(), watcher.to_owned()];
        self.transport.check_signers(&agents)?;
        self.transport.check_signers(&[registrar.into()])?;
        let params = self.parameters()?;
        ensure!(
            number(&params["blame_bond"])? == NEAR / 4
                && number(&params["challenge_bond"])? == NEAR / 4,
            "expected 0.25 NEAR bonds"
        );
        self.register(run, registrar, &agents)?;
        let task = json!({"assignment_seq":0,"worker":worker,"parent_assignment_seq":null});
        self.anchor(
            coord,
            "task_assigned",
            if matches!(scenario, "s5" | "s6") {
                MALFORMED
            } else {
                INPUT
            },
            task.clone(),
        )?;
        self.anchor(
            worker,
            "settlement",
            if scenario == "s3" { OUTPUT } else { WRONG },
            task,
        )?;
        if scenario == "s3" {
            let before = self.transport.snapshot(&agents)?;
            let chain = self.transport.view("get_run", json!({"run_id":run}))?;
            ensure!(
                self.transport
                    .view("verify_chain", json!({"run_id":run}))?
                    .is_null(),
                "stored chain broken"
            );
            ensure!(
                verify_journal(&self.journal, &chain)?.is_none(),
                "intact journal failed"
            );
            self.verify_run(run, None)?;
            let mut copy: Journal = serde_json::from_value(serde_json::to_value(&self.journal)?)?;
            copy.events[1]["payload_base64"] = json!(STANDARD.encode(WRONG));
            let path = self.path.as_ref().unwrap().with_extension("tampered.json");
            fs::write(&path, serde_json::to_vec_pretty(&copy)?)?;
            let mismatch = verify_journal(&copy, &chain)?.context("tamper not detected")?;
            ensure!(
                mismatch.seq == 1 && mismatch.signer == worker,
                "wrong tamper location"
            );
            ensure!(
                self.verify_run(run, Some(&path)).is_err(),
                "edited copy accepted"
            );
            let after = self.transport.snapshot(&agents)?;
            for id in &agents {
                ensure!(
                    before["accounts"][id]["amount"] == after["accounts"][id]["amount"],
                    "diagnostic changed balance"
                );
            }
            for field in ["reputations", "treasury"] {
                ensure!(before[field] == after[field], "diagnostic changed {field}");
            }
            ensure!(
                chain == self.transport.view("get_run", json!({"run_id":run}))?,
                "diagnostic mutated run"
            );
            self.journal.proof = Some(
                json!({"diagnostic_only":true,"mismatch":mismatch,"tampered_copy":path,"before":before,"after":after}),
            );
            self.save()?;
            println!("S3 PASS diagnostic only; MISMATCH at seq 1 signer={worker}");
            return Ok(());
        }
        let before = self.transport.snapshot(&agents)?;
        let begin = self.journal.transactions.len();
        self.journal.proof = Some(json!({"before":before}));
        self.save()?;
        let (reporter, blamed, class, defender) = if scenario == "s5" {
            (worker, coord, "F3", coord)
        } else {
            (coord, worker, "F1", challenger)
        };
        let id = self.file_claim(reporter, blamed, class, &[0, 1])?;
        let filed = self.transport.view("get_claim", json!({"claim_id":id}))?;
        let deadline = filed["challenge_deadline"]
            .as_u64()
            .context("missing deadline")?;
        ensure!(
            deadline
                - filed["claim_height"]
                    .as_u64()
                    .context("missing claim height")?
                == 600,
            "expected 600-block challenge window"
        );
        if scenario != "s1" {
            self.challenge(defender, id)?;
            let args = json!({"claim_id":id,"task_spec":STANDARD.encode(SPEC),"payloads":self.journal.events[..2].iter().map(|e| json!({"seq":e["seq"],"bytes":e["payload_base64"]})).collect::<Vec<_>>()});
            let preview = self.transport.view("preview_adjudication", args.clone())?;
            ensure!(
                preview["verdict"]
                    == if scenario == "s5" {
                        "Upheld"
                    } else {
                        "Overturned"
                    },
                "verdict differs from ground truth"
            );
            ensure!(
                preview["finding"]["agent"] == coord
                    && preview["finding"]["failure_class"] == "F3"
                    && preview["finding"]["origin"] == "Introduction",
                "wrong decisive finding"
            );
            ensure!(
                preview["origin"]
                    == if scenario == "s5" {
                        "Introduction"
                    } else {
                        "Propagation"
                    },
                "wrong causal origin"
            );
            self.journal.preview = Some(preview.clone());
            self.save()?;
            let persisted = self.submit(defender, "adjudicate", args, 0, false)?;
            ensure!(persisted.result == preview, "persisted preview differs");
            let adjudicated = self.transport.view("get_claim", json!({"claim_id":id}))?;
            ensure!(
                adjudicated["adjudication_result"] == preview
                    && adjudicated["corrected_attribution"] == coord,
                "stored finding differs"
            );
            ensure!(
                adjudicated["status"]
                    == if scenario == "s5" {
                        "AdjudicatedUpheld"
                    } else {
                        "AdjudicatedOverturned"
                    },
                "stored verdict differs"
            );
        }
        self.transport.wait_after(deadline)?;
        self.submit(reporter, "finalize", json!({"claim_id":id}), 0, false)?;
        let after = self.transport.snapshot(&agents)?;
        let final_claim = self.transport.view("get_claim", json!({"claim_id":id}))?;
        let final_run = self.transport.view("get_run", json!({"run_id":run}))?;
        self.journal.proof =
            Some(json!({"before":before,"after":after,"claim":final_claim,"run":final_run}));
        self.save()?;
        ensure!(
            final_claim["status"] == "Finalized"
                && number(&final_claim["bond"])? == 0
                && number(&final_claim["challenge_bond"])? == 0,
            "bonds not finalized"
        );
        ensure!(
            final_claim["reporter"] == reporter
                && final_claim["blamed"] == blamed
                && final_claim["failure_class"] == class,
            "finalized claim differs from seeded ground truth"
        );
        if scenario == "s1" {
            ensure!(
                final_claim["adjudication_result"].is_null(),
                "S1 must remain unchallenged"
            );
        }
        let verification = verify_seeded_money(
            scenario,
            &before,
            &after,
            &agents,
            defender,
            &self.journal.transactions[begin..],
        )?;
        for agent in final_run["agents"].as_array().context("missing stakes")? {
            let slash = scenario != "s6" && agent["account_id"] == blamed;
            ensure!(
                number(&agent["locked_stake"])? == if slash { NEAR * 3 / 4 } else { NEAR },
                "incorrect stake slash"
            );
        }
        ensure!(
            final_claim["corrected_attribution"]
                == if scenario == "s1" {
                    Value::Null
                } else {
                    json!(coord)
                },
            "corrected attribution changed"
        );
        self.journal.proof = Some(
            json!({"before":before,"after":after,"claim":final_claim,"run":final_run,"verification":verification,"elapsed_ms":start.elapsed().as_millis(),"ground_truth":if scenario == "s1" { "F1 worker; optimistic acceptance" } else { "F3 coordinator" }}),
        );
        self.save()?;
        println!(
            "{} PASS state, economic balances, stakes and baseline-relative reputation verified; elapsed_ms={} Evidence: {}",
            scenario.to_uppercase(),
            start.elapsed().as_millis(),
            self.path.as_ref().unwrap().display()
        );
        Ok(())
    }
}

fn verify_seeded_money(
    scenario: &str,
    before: &Value,
    after: &Value,
    agents: &[String],
    challenger: &str,
    txs: &[Tx],
) -> Result<Value> {
    let n = NEAR as i128;
    let expected = match scenario {
        "s1" => [n / 5, 0, 0], // Sponsor's locked stake loses 0.25 separately.
        "s5" => [-n / 4, n * 2 / 5, 0],
        "s6" if challenger == agents[1] => [-n / 4, n / 5, 0],
        "s6" => [-n / 4, 0, n / 5],
        _ => bail!("unsupported settlement"),
    };
    let mut rows = Vec::new();
    for (i, id) in agents.iter().enumerate() {
        let delta = i128::try_from(number(&after["accounts"][id]["amount"])?)?
            - i128::try_from(number(&before["accounts"][id]["amount"])?)?;
        let costs =
            txs.iter()
                .filter(|t| &t.signer == id)
                .try_fold(0u128, |sum, t| -> Result<u128> {
                    sum.checked_add(t.gas_yocto.parse()?)
                        .and_then(|n| n.checked_add(t.storage_yocto.parse().ok()?))
                        .context("cost overflow")
                })?;
        let economic = delta + i128::try_from(costs)?;
        ensure!(
            economic == expected[i],
            "{id} economic mismatch: {economic} expected {}",
            expected[i]
        );
        let base = before["reputations"][id]
            .as_u64()
            .context("missing reputation")?;
        let wanted = match (scenario, i) {
            ("s1", 1) | ("s5", 0) => base.saturating_sub(10),
            ("s6", 0) => base.saturating_sub(15),
            ("s6", 1) if challenger == id => ((base + 5).min(120) + 2).min(120),
            ("s6", 1) => (base + 2).min(120),
            ("s6", 2) if challenger == id => (base + 5).min(120),
            _ => base,
        };
        ensure!(
            after["reputations"][id] == wanted,
            "{id} reputation mismatch"
        );
        rows.push(json!({"account":id,"economic_delta_yocto":economic.to_string(),"gas_and_storage_yocto":costs.to_string(),"reputation_before":base,"reputation_after":wanted}));
    }
    let treasury = number(&after["treasury"])? - number(&before["treasury"])?;
    ensure!(
        treasury
            == if scenario == "s5" {
                NEAR / 10
            } else {
                NEAR / 20
            },
        "treasury mismatch"
    );
    let payout = if scenario == "s5" {
        NEAR * 65 / 100
    } else {
        NEAR * 45 / 100
    };
    ensure!(
        payout + treasury
            == if scenario == "s5" {
                NEAR * 3 / 4
            } else {
                NEAR / 2
            },
        "conservation failed"
    );
    Ok(
        json!({"roles":rows,"treasury_delta_yocto":treasury.to_string(),"payout_yocto":payout.to_string(),"stake_slash_yocto":if scenario == "s6" { "0".into() } else { (NEAR/4).to_string() }}),
    )
}
