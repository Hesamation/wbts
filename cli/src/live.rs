use crate::{App, Transport, Tx, account, number};
use anyhow::{Context, Result, anyhow, bail, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use near_crypto::SecretKey;
use near_primitives::{
    action::{Action, FunctionCallAction},
    transaction::{SignedTransaction, Transaction, TransactionV0},
};
use reqwest::blocking::Client;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub struct Live {
    client: Client,
    url: String,
    contract: String,
    dir: PathBuf,
    keys: BTreeMap<String, SecretKey>,
    storage_byte_cost: u128,
}

impl Live {
    pub fn connect(app: &App) -> Result<Self> {
        let mut live = Self {
            client: Client::builder().timeout(Duration::from_secs(45)).build()?,
            url: app.rpc_url.clone(),
            contract: app.contract.clone(),
            dir: app.journal_dir.join("transactions"),
            keys: BTreeMap::new(),
            storage_byte_cost: 0,
        };
        let status = live.rpc("status", json!([]))?;
        ensure!(
            status["chain_id"] == "testnet",
            "RPC must report chain_id=testnet; refusing to sign"
        );
        let config = live.rpc("EXPERIMENTAL_protocol_config", json!({"finality":"final"}))?;
        live.storage_byte_cost = number(&config["runtime_config"]["storage_amount_per_byte"])?;
        fs::create_dir_all(&live.dir)?;
        Ok(live)
    }
    fn rpc(&self, method: &str, params: Value) -> Result<Value> {
        // Retry read-only RPCs only. A send with an uncertain outcome is never resubmitted.
        let attempts = if method == "send_tx" { 1 } else { 3 };
        let mut error = anyhow!("RPC failed");
        for attempt in 0..attempts {
            let response =
                (|| -> Result<Value> {
                    let response: Value = self.client.post(&self.url)
                    .json(&json!({"jsonrpc":"2.0","id":"wbts-cli","method":method,"params":params}))
                    .send()?.error_for_status()?.json()?;
                    if let Some(e) = response.get("error") {
                        bail!("RPC {method}: {e}");
                    }
                    response
                        .get("result")
                        .cloned()
                        .context("RPC missing result")
                })();
            match response {
                Ok(value) => return Ok(value),
                Err(e) => error = e,
            }
            if attempt + 1 < attempts {
                thread::sleep(Duration::from_secs(1 << attempt));
            }
        }
        Err(error).with_context(|| format!("RPC {method}"))
    }
    fn query_account(&self, id: &str, block: Option<u64>) -> Result<Value> {
        let mut args = json!({"request_type":"view_account","account_id":id});
        if let Some(block) = block {
            args["block_id"] = json!(block);
        } else {
            args["finality"] = json!("final");
        }
        self.rpc("query", args)
    }
    fn view_at(&self, method: &str, args: Value, block: Option<u64>) -> Result<Value> {
        let mut params = json!({"request_type":"call_function","account_id":self.contract,"method_name":method,"args_base64":STANDARD.encode(serde_json::to_vec(&args)?)});
        if let Some(block) = block {
            params["block_id"] = json!(block);
        } else {
            params["finality"] = json!("final");
        }
        let response = self.rpc("query", params)?;
        let bytes: Vec<u8> = serde_json::from_value(response["result"].clone())?;
        serde_json::from_slice(&bytes).context("invalid view JSON")
    }
    fn key(&mut self, id: &str) -> Result<()> {
        account(id)?;
        if self.keys.contains_key(id) {
            return Ok(());
        }
        let keys = self.rpc(
            "query",
            json!({"request_type":"view_access_key_list","account_id":id,"finality":"final"}),
        )?;
        for entry in keys["keys"].as_array().context("missing access keys")? {
            if entry["access_key"]["permission"] != "FullAccess" {
                continue;
            }
            let public = entry["public_key"].as_str().context("missing public key")?;
            let credential =
                keyring::Entry::new(&format!("near-testnet-{id}"), &format!("{id}:{public}"))
                    .ok()
                    .and_then(|e| e.get_password().ok());
            if let Some(credential) = credential
                && let Ok(key) = decode_key(&credential, public)
            {
                self.keys.insert(id.into(), key);
                return Ok(());
            }
            let root = PathBuf::from(std::env::var_os("HOME").context("HOME missing")?)
                .join(".near-credentials/testnet");
            for path in [
                root.join(format!("{id}.json")),
                root.join(id).join(format!("{public}.json")),
            ] {
                if let Ok(credential) = fs::read_to_string(path)
                    && let Ok(key) = decode_key(&credential, public)
                {
                    self.keys.insert(id.into(), key);
                    return Ok(());
                }
            }
        }
        bail!(
            "no matching full-access key for {id} in NEAR secure keychain or ~/.near-credentials/testnet; CLI never creates keys"
        )
    }
    fn write_public(&self, hash: &str, data: &Value) -> Result<()> {
        let path = self.dir.join(format!("{hash}.json"));
        atomic_json(&path, data)
    }
}

// No deserializer errors, secret strings, or SecretKey Debug values escape this function.
fn decode_key(credential: &str, public: &str) -> Result<SecretKey> {
    let value: Value =
        serde_json::from_str(credential).map_err(|_| anyhow!("invalid credential"))?;
    let key: SecretKey = value["private_key"]
        .as_str()
        .ok_or_else(|| anyhow!("missing key"))?
        .parse()
        .map_err(|_| anyhow!("invalid key"))?;
    ensure!(key.public_key().to_string() == public, "key mismatch");
    Ok(key)
}

fn atomic_json(path: &Path, data: &Value) -> Result<()> {
    let temp = path.with_extension(format!("{}.tmp", std::process::id()));
    fs::write(&temp, serde_json::to_vec_pretty(data)?)?;
    fs::rename(temp, path)?;
    Ok(())
}

impl Transport for Live {
    fn check_signers(&mut self, accounts: &[String]) -> Result<()> {
        for id in accounts {
            self.key(id)?;
        }
        Ok(())
    }
    fn view(&mut self, method: &str, args: Value) -> Result<Value> {
        self.view_at(method, args, None)
    }
    fn snapshot(&mut self, accounts: &[String]) -> Result<Value> {
        let block = self.rpc("block", json!({"finality":"final"}))?;
        let height = block["header"]["height"]
            .as_u64()
            .context("missing block height")?;
        let mut balances = serde_json::Map::new();
        let mut reps = serde_json::Map::new();
        for id in accounts {
            balances.insert(id.clone(), self.query_account(id, Some(height))?);
            reps.insert(
                id.clone(),
                self.view_at("get_reputation", json!({"agent":id}), Some(height))?,
            );
        }
        Ok(
            json!({"block_height":height,"block_hash":block["header"]["hash"],"accounts":balances,"reputations":reps,"treasury":self.view_at("get_treasury",json!({}),Some(height))?}),
        )
    }
    fn wait_after(&mut self, deadline: u64) -> Result<()> {
        let start = Instant::now();
        loop {
            let block = self.rpc("block", json!({"finality":"final"}))?;
            let height = block["header"]["height"]
                .as_u64()
                .context("missing block height")?;
            println!(
                "Waiting for challenge window: finalized_block={height} deadline={deadline} elapsed_s={}",
                start.elapsed().as_secs()
            );
            if height > deadline {
                return Ok(());
            }
            ensure!(
                start.elapsed() < Duration::from_secs(1800),
                "challenge-window wait exceeded 30 minutes; no finalize submitted"
            );
            thread::sleep(Duration::from_secs(10));
        }
    }
    fn call(
        &mut self,
        signer: &str,
        method: &str,
        args: Value,
        principal: u128,
        allowance: u128,
    ) -> Result<Tx> {
        account(signer)?;
        // Recheck chain identity before every signature, including after a long wait.
        ensure!(
            self.rpc("status", json!([]))?["chain_id"] == "testnet",
            "RPC chain changed; refusing signature"
        );
        self.key(signer)?;
        let key = &self.keys[signer];
        let access = self.rpc("query",json!({"request_type":"view_access_key","account_id":signer,"public_key":key.public_key(),"finality":"final"}))?;
        ensure!(
            access["permission"] == "FullAccess",
            "full-access key required"
        );
        let before = self.query_account(&self.contract, None)?;
        let signer_before = self.query_account(signer, None)?;
        let deposit = principal
            .checked_add(allowance)
            .context("deposit overflow")?;
        let available = number(&signer_before["amount"])?;
        let account_storage = u128::from(
            signer_before["storage_usage"]
                .as_u64()
                .context("missing signer storage")?,
        )
        .checked_mul(self.storage_byte_cost)
        .context("account storage overflow")?;
        ensure!(
            available
                >= deposit
                    .checked_add(account_storage)
                    .and_then(|n| n.checked_add(crate::NEAR / 100))
                    .context("funding overflow")?,
            "{signer} has insufficient spendable balance for {method}: attached deposit plus account storage and 0.01 NEAR gas headroom required; no transaction submitted"
        );
        let tx = Transaction::V0(TransactionV0 {
            signer_id: signer.parse()?,
            public_key: key.public_key(),
            nonce: access["nonce"]
                .as_u64()
                .context("missing nonce")?
                .checked_add(1)
                .context("nonce overflow")?,
            receiver_id: self.contract.parse()?,
            block_hash: access["block_hash"]
                .as_str()
                .context("missing block hash")?
                .parse()
                .map_err(|_| anyhow!("invalid block hash"))?,
            actions: vec![Action::FunctionCall(Box::new(FunctionCallAction {
                method_name: method.into(),
                args: serde_json::to_vec(&args)?,
                gas: near_primitives::types::Gas::from_gas(100_000_000_000_000),
                deposit: near_primitives::types::Balance::from_yoctonear(deposit),
            }))],
        });
        let signature = key.sign(tx.get_hash_and_size().0.as_ref());
        let signed = SignedTransaction::new(signature, tx);
        let hash = signed.get_hash().to_string();
        let started = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
        let start = Instant::now();
        let mut record = json!({"hash":hash,"signer":signer,"contract":self.contract,"method":method,"args":args,"attached_yocto":deposit.to_string(),"started_unix_ms":started,"state":"submission-intent"});
        self.write_public(&hash, &record)?;
        println!("Submitting {method} tx={hash} (intent saved before broadcast)");
        let send = self.rpc("send_tx",json!({"signed_tx_base64":STANDARD.encode(borsh::to_vec(&signed)?),"wait_until":"FINAL"}));
        let outcome = match send {
            Ok(outcome) => outcome,
            Err(e) => {
                // Preserve hash even if broadcast succeeded but its HTTP response was lost.
                record["state"] = json!("submission-uncertain");
                record["error"] = json!(format!("{e:#}"));
                self.write_public(&hash, &record)?;
                bail!(
                    "{method} tx={hash}: submission outcome uncertain; inspect public transaction record and chain; do not retry automatically"
                );
            }
        };
        record["outcome"] = outcome.clone();
        record["state"] = json!("received");
        self.write_public(&hash, &record)?;
        ensure!(
            outcome["final_execution_status"] == "FINAL",
            "{method} tx={hash} did not reach FINAL"
        );
        ensure!(
            outcome["transaction"]["hash"] == hash,
            "RPC transaction hash mismatch for {hash}"
        );
        ensure!(
            outcome["status"].get("SuccessValue").is_some(),
            "{method} tx={hash} execution failed: {}",
            outcome["status"]
        );
        let receipts = outcome["receipts_outcome"]
            .as_array()
            .context("missing receipt outcomes")?;
        for receipt in receipts {
            ensure!(
                receipt["outcome"]["status"].get("Failure").is_none(),
                "{method} tx={hash} failed receipt: {}",
                receipt["outcome"]["status"]
            );
        }
        let execution = execution_receipt(&outcome, &self.contract)?;
        let block = self.rpc("block", json!({"block_id":execution["block_hash"]}))?;
        let height = block["header"]["height"]
            .as_u64()
            .context("missing execution height")?;
        let after = self.query_account(&self.contract, None)?;
        let storage_bytes = after["storage_usage"]
            .as_u64()
            .context("missing storage")?
            .checked_sub(
                before["storage_usage"]
                    .as_u64()
                    .context("missing initial storage")?,
            )
            .context("unexpected storage decrease")?;
        let storage = u128::from(storage_bytes)
            .checked_mul(self.storage_byte_cost)
            .context("storage overflow")?;
        ensure!(
            storage <= allowance,
            "storage grew more than funded allowance; concurrent contract activity invalidates measurement"
        );
        let mut gas = 0u64;
        let mut cost = 0u128;
        for o in std::iter::once(&outcome["transaction_outcome"]["outcome"])
            .chain(receipts.iter().map(|r| &r["outcome"]))
        {
            gas = gas
                .checked_add(o["gas_burnt"].as_u64().context("missing gas")?)
                .context("gas overflow")?;
            cost = cost
                .checked_add(number(&o["tokens_burnt"])?)
                .context("gas cost overflow")?;
        }
        let value = STANDARD.decode(
            outcome["status"]["SuccessValue"]
                .as_str()
                .context("missing success value")?,
        )?;
        let result = if value.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&value)?
        };
        if allowance > 0 {
            let signer_after = self.query_account(signer, None)?;
            let actual_spent = available.checked_sub(number(&signer_after["amount"])?).context("unexpected signer balance increase; concurrent activity invalidates accounting")?;
            ensure!(
                actual_spent
                    == principal
                        .checked_add(storage)
                        .and_then(|n| n.checked_add(cost))
                        .context("accounting overflow")?,
                "{method} tx={hash} refund/balance reconciliation failed; inspect receipts and concurrent activity"
            );
        }
        let tx = Tx {
            hash: hash.clone(),
            signer: signer.into(),
            method: method.into(),
            block_height: height,
            elapsed_ms: start.elapsed().as_millis(),
            started_unix_ms: started,
            ended_unix_ms: SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),
            gas_burnt: gas,
            gas_yocto: cost.to_string(),
            storage_bytes,
            storage_yocto: storage.to_string(),
            attached_yocto: deposit.to_string(),
            excess_refunded_yocto: (allowance - storage).to_string(),
            result,
            outcome,
        };
        self.write_public(&hash, &serde_json::to_value(&tx)?)?;
        Ok(tx)
    }
}

fn execution_receipt<'a>(outcome: &'a Value, contract: &str) -> Result<&'a Value> {
    let ids = outcome["transaction_outcome"]["outcome"]["receipt_ids"]
        .as_array()
        .context("missing transaction receipt IDs")?;
    ensure!(
        ids.len() == 1,
        "single-call transaction must create one root receipt"
    );
    let receipt = outcome["receipts_outcome"]
        .as_array()
        .context("missing receipt outcomes")?
        .iter()
        .find(|r| r["id"] == ids[0])
        .context("missing root contract execution receipt")?;
    ensure!(
        receipt["outcome"]["executor_id"] == contract,
        "root receipt executor mismatch"
    );
    let status = &receipt["outcome"]["status"];
    ensure!(
        status.get("SuccessValue").is_some() || status.get("SuccessReceiptId").is_some(),
        "root contract execution receipt did not succeed"
    );
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use std::io::{Read, Write};

    #[test]
    fn finalize_promise_return_selects_contract_receipt_instead_of_payout() {
        let mut outcome: Value =
            serde_json::from_str(include_str!("../tests/fixtures/finalize-receipts.json")).unwrap();
        let root = execution_receipt(&outcome, "s2.coord-wbts.testnet").unwrap();
        assert_eq!(
            root["id"],
            outcome["transaction_outcome"]["outcome"]["receipt_ids"][0]
        );
        assert!(root["outcome"]["status"].get("SuccessReceiptId").is_some());
        outcome["receipts_outcome"][0]["outcome"]["status"] = json!({"SuccessValue":""});
        assert!(execution_receipt(&outcome, "s2.coord-wbts.testnet").is_ok());
        outcome["receipts_outcome"][0]["outcome"]["executor_id"] = json!("unexpected.testnet");
        assert!(execution_receipt(&outcome, "s2.coord-wbts.testnet").is_err());
    }

    #[test]
    fn mainnet_rpc_is_rejected_before_signing_or_creating_journals() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 4096];
            let _ = stream.read(&mut request).unwrap();
            let body = r#"{"jsonrpc":"2.0","id":"wbts-cli","result":{"chain_id":"mainnet"}}"#;
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        });
        let app = App::parse_from([
            "cli",
            "--rpc-url",
            &format!("http://{address}"),
            "run-demo",
            "s2",
        ]);
        let error = Live::connect(&app).err().unwrap();
        assert!(error.to_string().contains("chain_id=testnet"));
        server.join().unwrap();
    }
    #[test]
    fn malformed_credentials_do_not_echo_secrets() {
        let sentinel = "PRIVATE-MATERIAL-SENTINEL";
        for credential in [
            sentinel.to_string(),
            format!(r#"{{"private_key":"{sentinel}"}}"#),
        ] {
            let error = decode_key(&credential, "ed25519:unused").err().unwrap();
            assert!(!format!("{error:#}").contains(sentinel));
        }
    }
}
